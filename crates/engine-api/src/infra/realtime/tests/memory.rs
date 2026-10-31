use super::*;

pub(crate) async fn memory() -> Arc<Datastore> {
    Arc::new(
        Datastore::open("mem://", "live", "live", None, None, KvBudget::default())
            .await
            .expect("the memory store opens"),
    )
}
pub(crate) async fn insert_user(db: &Datastore, game: Uuid, username: &str) {
    let params = std::collections::BTreeMap::from([
        ("game".to_string(), serde_json::json!(game.to_string())),
        ("username".to_string(), serde_json::json!(username)),
    ]);
    db.exec_params("CREATE users SET game_id = $game, username = $username", &params)
        .await
        .expect("a user row is created");
}
pub(crate) async fn create_user(db: &Datastore, game: Uuid, username: &str) -> Uuid {
    let id = Uuid::new_v4();
    let params = std::collections::BTreeMap::from([
        ("id".to_string(), serde_json::json!(id.to_string())),
        ("game".to_string(), serde_json::json!(game.to_string())),
        ("username".to_string(), serde_json::json!(username)),
    ]);
    db.exec_params(
        "CREATE type::record('users', <uuid>$id) SET game_id = $game, username = $username",
        &params,
    )
    .await
    .expect("a user row with a known id is created");
    id
}
pub(crate) async fn next(sub: &mut Subscription) -> FeedEvent {
    tokio::time::timeout(Duration::from_secs(5), sub.recv())
        .await
        .expect("a live event within 5 s")
        .expect("the topic stays open")
}
#[tokio::test]
pub(crate) async fn one_query_feeds_every_subscriber_of_the_tenant() {
    let db = memory().await;
    db.exec("DEFINE TABLE IF NOT EXISTS users SCHEMALESS")
        .await
        .expect("the users table exists");
    let hub = RealtimeHub::new(Some(Arc::clone(&db)));
    let game = Uuid::new_v4();
    let other = Uuid::new_v4();

    let mut first = hub.subscribe(LiveTopic::Users, game).await.expect("subscribed");
    let mut second = hub.subscribe(LiveTopic::Users, game).await.expect("subscribed");
    assert_eq!(hub.topic_count().await, 1, "both sockets share one live query");

    insert_user(&db, other, "someone-else").await;
    insert_user(&db, game, "player").await;

    let event = next(&mut first).await;
    assert_eq!(event.topic, "users");
    assert_eq!(event.action, "create");
    assert_eq!(event.seq, 1);
    let same = next(&mut second).await;
    assert_eq!(same.record, event.record, "both subscribers see the same row");

    let params = std::collections::BTreeMap::from([
        ("game".to_string(), serde_json::json!(game.to_string())),
    ]);
    db.exec_params(
        "UPDATE users SET username = $name WHERE game_id = $game",
        &std::collections::BTreeMap::from([
            ("game".to_string(), serde_json::json!(game.to_string())),
            ("name".to_string(), serde_json::json!("renamed")),
        ]),
    )
    .await
    .expect("the row is updated");
    let updated = next(&mut first).await;
    assert_eq!(updated.action, "update");
    assert_eq!(updated.record, event.record);
    assert_eq!(updated.seq, 2);

    db.exec_params(
        "DELETE users WHERE game_id = $game",
        &params,
    )
    .await
    .expect("the row is deleted");
    let deleted = next(&mut first).await;
    assert_eq!(deleted.action, "delete");
    assert_eq!(deleted.record, event.record);
    assert_eq!(deleted.seq, 3);
}
#[tokio::test]
pub(crate) async fn the_last_subscriber_leaving_kills_the_query() {
    let db = memory().await;
    db.exec("DEFINE TABLE IF NOT EXISTS users SCHEMALESS")
        .await
        .expect("the users table exists");
    let hub = RealtimeHub::new(Some(Arc::clone(&db)));
    let game = Uuid::new_v4();

    let first = hub.subscribe(LiveTopic::Users, game).await.expect("subscribed");
    let second = hub.subscribe(LiveTopic::Users, game).await.expect("subscribed");
    drop(first);
    assert_eq!(hub.topic_count().await, 1, "one subscriber still watches");

    drop(second);
    tokio::time::timeout(Duration::from_secs(5), async {
        while hub.topic_count().await != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the retired query leaves the registry");

    let mut third = hub.subscribe(LiveTopic::Users, game).await.expect("resubscribed");
    assert_eq!(hub.topic_count().await, 1);
    insert_user(&db, game, "after-restart").await;
    assert_eq!(next(&mut third).await.action, "create");
}
#[tokio::test]
pub(crate) async fn without_a_datastore_there_is_nothing_to_watch() {
    let hub = RealtimeHub::new(None);
    let err = hub
        .subscribe(LiveTopic::Users, Uuid::new_v4())
        .await
        .expect_err("no datastore, no feed");
    assert!(matches!(err, HubError::Unavailable));
}
#[test]
pub(crate) fn every_topic_is_named_once_and_bound_to_its_tenant() {
    let mut names = std::collections::HashSet::new();
    for topic in LiveTopic::ALL {
        assert!(names.insert(topic.name()), "{} is claimed twice", topic.name());
        assert!(
            topic.statement().contains("WHERE game_id = $game"),
            "{} must filter on the bound tenant",
            topic.name()
        );
    }
    assert_eq!(
        LiveTopic::ALL.len(),
        names.len(),
        "ALL must name every topic exactly once"
    );
}
