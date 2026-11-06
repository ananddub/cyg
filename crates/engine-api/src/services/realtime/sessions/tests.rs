use super::*;
use crate::infra::ws::envelope::Envelope;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

fn registered_socket(uid: Uuid, name: &str) -> (Arc<Session>, UnboundedReceiver<String>) {
    let (tx, rx) = unbounded_channel::<String>();
    let session = Session::new(Uuid::new_v4(), uid, name.to_owned(), tx);
    RealtimeSessionHub::global().register(session.clone());
    (session, rx)
}

#[test]
fn kick_user_forgets_the_sessions_and_closes_the_sockets() {
    let uid = Uuid::new_v4();
    let (session_a, mut rx_a) = registered_socket(uid, "a");
    let (_session_b, _rx_b) = registered_socket(uid, "b");
    let (stranger, _rx_stranger) = registered_socket(Uuid::new_v4(), "stranger");

    let kicked = RealtimeSessionHub::global().kick_user(uid, "Banned: test");
    assert_eq!(kicked, 2, "both sockets of the user go");
    assert!(
        RealtimeSessionHub::global().sessions_of(uid).is_empty(),
        "the registry forgets the user"
    );
    assert_eq!(
        RealtimeSessionHub::global().sessions_of(stranger.uid).len(),
        1
    );

    for rx in [&mut rx_a] {
        let frame = rx.blocking_recv().expect("the error frame arrives");
        assert!(frame.contains("\"error\""), "frame: {frame}");
        assert!(frame.contains("Banned"), "frame: {frame}");
        assert!(
            rx.blocking_recv().is_none(),
            "the socket is closed after the frame"
        );
    }
    let _ = session_a;
}

#[test]
fn kick_user_leaves_every_match_with_the_presence_event() {
    let uid = Uuid::new_v4();
    let (session, mut rx) = registered_socket(uid, "member");
    let matches = crate::services::realtime::RealtimeMatchService::global();
    let match_id = matches.create_match(&session, true, 30, "kick-test".into());
    matches
        .join_match(&session, Some(match_id.to_string()), None, None, None)
        .expect("the join lands");
    assert!(matches.match_exists(match_id));

    let (bystander, mut rx_bystander) = registered_socket(Uuid::new_v4(), "bystander");
    matches
        .join_match(&bystander, Some(match_id.to_string()), None, None, None)
        .expect("the second join lands");

    let kicked = RealtimeSessionHub::global().kick_user(uid, "Banned");
    assert_eq!(kicked, 1);

    if let Some(snapshot) =
        crate::services::realtime::RealtimeMatchService::get_snapshot(match_id)
    {
        assert!(
            snapshot
                .members
                .iter()
                .all(|m| m.user_id != uid.to_string()),
            "the kicked member is gone: {:?}",
            snapshot.members
        );
    }
    let frames: Vec<String> = std::iter::from_fn(|| rx.blocking_recv()).collect();
    assert!(
        frames.iter().any(|f| f.contains("\"error\"")),
        "the ban error frame rides: {frames:?}"
    );
    let seen: Vec<String> = (0..2)
        .filter_map(|_| rx_bystander.blocking_recv())
        .collect();
    assert!(
        seen.iter()
            .any(|f| f.contains("match_presence") && f.contains("leaves")),
        "the presence leave rides to the remaining member: {seen:?}"
    );
}

#[test]
fn kick_of_an_offline_user_is_a_quiet_zero() {
    let uid = Uuid::new_v4();
    assert_eq!(RealtimeSessionHub::global().kick_user(uid, "Banned"), 0);
    let (tx, rx) = unbounded_channel::<String>();
    let session = Session::new(Uuid::new_v4(), uid, "dying".into(), tx);
    session.close();
    drop(rx);
    session.send(&Envelope {
        cid: None,
        message: crate::infra::ws::envelope::EnvelopeMessage::Ping,
    });
}
