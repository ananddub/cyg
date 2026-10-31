use std::collections::BTreeMap;
use std::time::Duration;

use futures::StreamExt;
use tokio::net::TcpListener;
use uuid::Uuid;

use super::live_socket::console_token;
use crate::db::Datastore;
use crate::serve;
use crate::tests::games::db;

#[tokio::test]
pub(crate) async fn a_scoped_socket_sees_its_tenants_rows_and_an_unscoped_one_sees_none() {
    use tokio_tungstenite::connect_async;

    let Some((state, store)) = db::state().await else {
        return;
    };
    let hosted = db::create(&state, "live-feed").await;
    let other = db::create(&state, "live-other").await;
    store
        .exec("DEFINE TABLE IF NOT EXISTS users SCHEMALESS")
        .await
        .expect("the users table exists");
    store
        .exec("DEFINE TABLE IF NOT EXISTS purchase SCHEMALESS")
        .await
        .expect("the purchase table exists");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ephemeral bind");
    let port = listener.local_addr().expect("bound addr").port();
    let server = tokio::spawn(serve(listener, state, std::future::pending()));

    let url = format!(
        "ws://127.0.0.1:{port}/ws/dashboard?token={}&game_id={}",
        console_token(),
        hosted.id
    );
    let (mut scoped, _) = connect_async(&url)
        .await
        .expect("dashboard socket connects");
    assert_eq!(next_json(&mut scoped).await["type"].as_str(), Some("hello"));
    assert_eq!(
        next_json(&mut scoped).await["type"].as_str(),
        Some("snapshot")
    );

    insert_user(&store, other.id, "someone-else").await;
    let player = insert_user(&store, hosted.id, "player").await;

    let frame = next_of_type(&mut scoped, "live").await;
    assert_eq!(frame["topic"].as_str(), Some("users"));
    assert_eq!(frame["action"].as_str(), Some("create"));
    assert_eq!(frame["record"].as_str(), Some(player.as_str()));
    assert_eq!(frame["seq"].as_u64(), Some(1), "the tenant's first change");

    insert_purchase(&store, other.id, "other-txn").await;
    insert_purchase(&store, hosted.id, "hosted-txn").await;
    let frame = next_of_type(&mut scoped, "live").await;
    assert_eq!(frame["topic"].as_str(), Some("purchases"));
    assert_eq!(frame["action"].as_str(), Some("create"));
    assert_eq!(frame["record"].as_str(), Some("hosted-txn"));
    assert_eq!(
        frame["seq"].as_u64(),
        Some(1),
        "each topic counts on its own"
    );

    let (mut unscoped, _) = connect_async(format!(
        "ws://127.0.0.1:{port}/ws/dashboard?token={}",
        console_token()
    ))
    .await
    .expect("unscoped socket connects");
    assert_eq!(
        next_json(&mut unscoped).await["type"].as_str(),
        Some("hello")
    );
    insert_user(&store, hosted.id, "second-player").await;
    let mut snapshots = 0;
    while snapshots < 2 {
        let frame = next_json(&mut unscoped).await;
        match frame["type"].as_str() {
            Some("snapshot") => snapshots += 1,
            Some("live") => {
                assert_eq!(
                    frame["action"].as_str(),
                    Some("touch"),
                    "an unscoped socket may learn that a table moved"
                );
                assert_eq!(
                    frame["record"].as_str(),
                    Some(""),
                    "but a mark never carries a tenant's row"
                );
            }
            _ => {}
        }
    }

    server.abort();
}

#[tokio::test]
pub(crate) async fn a_change_off_the_account_plane_still_wakes_the_socket() {
    use tokio_tungstenite::connect_async;

    let Some((state, store)) = db::state().await else {
        return;
    };
    let hosted = db::create(&state, "live-groups").await;
    store
        .exec("DEFINE TABLE IF NOT EXISTS groups SCHEMALESS")
        .await
        .expect("the groups table exists");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ephemeral bind");
    let port = listener.local_addr().expect("bound addr").port();
    let server = tokio::spawn(serve(listener, state, std::future::pending()));

    let url = format!(
        "ws://127.0.0.1:{port}/ws/dashboard?token={}&game_id={}",
        console_token(),
        hosted.id
    );
    let (mut scoped, _) = connect_async(&url)
        .await
        .expect("dashboard socket connects");
    assert_eq!(next_json(&mut scoped).await["type"].as_str(), Some("hello"));
    assert_eq!(
        next_json(&mut scoped).await["type"].as_str(),
        Some("snapshot")
    );

    store
        .exec("CREATE groups SET name = 'raiders'")
        .await
        .expect("a group row is created");

    let frame = next_of_type(&mut scoped, "live").await;
    assert_eq!(frame["topic"].as_str(), Some("groups"));
    assert_eq!(frame["action"].as_str(), Some("touch"));
    assert_eq!(frame["record"].as_str(), Some(""));
    assert_eq!(frame["seq"].as_u64(), Some(1));

    server.abort();
}

pub(crate) async fn insert_user(store: &Datastore, game: Uuid, username: &str) -> String {
    let params = BTreeMap::from([
        ("game".to_string(), serde_json::json!(game.to_string())),
        ("username".to_string(), serde_json::json!(username)),
    ]);
    let rows: Vec<serde_json::Value> = store
        .query_params(
            "CREATE users SET game_id = $game, username = $username RETURN AFTER",
            &params,
        )
        .await
        .expect("a user row is created");
    rows[0]["id"]
        .as_str()
        .expect("the new row carries its id")
        .to_string()
}

pub(crate) async fn insert_purchase(store: &Datastore, game: Uuid, transaction_id: &str) {
    let params = BTreeMap::from([
        ("game".to_string(), serde_json::json!(game.to_string())),
        (
            "transaction_id".to_string(),
            serde_json::json!(transaction_id),
        ),
    ]);
    store
        .exec_params(
            "CREATE type::record('purchase', $transaction_id) SET game_id = $game",
            &params,
        )
        .await
        .expect("a purchase row is created");
}

pub(crate) async fn next_of_type<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
    kind: &str,
) -> serde_json::Value
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        let frame = next_json(ws).await;
        if frame["type"].as_str() == Some(kind) {
            return frame;
        }
    }
}

pub(crate) async fn next_json<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
) -> serde_json::Value
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio_tungstenite::tungstenite::Message as Raw;
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .expect("a frame within 10 s")
            .expect("socket stays open")
            .expect("tungstenite");
        if let Raw::Text(text) = msg {
            return serde_json::from_str(text.as_str()).expect("frame is JSON");
        }
    }
}
