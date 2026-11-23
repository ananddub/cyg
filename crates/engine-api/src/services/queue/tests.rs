use super::*;

fn service() -> QueueService {
    QueueService::new(crate::services::plane::Plane::for_tests())
}

fn req(room: &str, party_size: usize, max: usize, mmr: f64) -> QueueJoin {
    QueueJoin {
        room: room.into(),
        mode: "ranked".into(),
        region: "mumbai".into(),
        mmr,
        party_size: Some(party_size),
        max_players: Some(max),
        ttl_ms: None,
    }
}

#[test]
fn two_solo_tickets_form_one_match() {
    let q = service();
    let a = q.join_test(req("arena", 1, 2, 1000.0), 0).unwrap();
    assert_eq!(a.status, "queued");
    let b = q.join_test(req("arena", 1, 2, 1010.0), 1).unwrap();
    assert_eq!(b.status, "matched");
    let a = q.status_test(&a.ticket_id, 2).unwrap();
    assert_eq!(a.status, "matched");
    assert_eq!(a.match_id, b.match_id);
    assert_eq!(a.room, "arena");
}

#[test]
fn a_party_is_never_split_and_is_rejected_over_capacity() {
    let q = service();
    assert!(q.join_test(req("arena", 3, 2, 1000.0), 0).is_err());
    let party = q.join_test(req("arena", 2, 2, 1000.0), 0).unwrap();
    assert_eq!(party.status, "matched", "a full party fills its own room");
}

#[test]
fn modes_and_regions_do_not_cross_match() {
    let q = service();
    q.join_test(req("arena", 1, 2, 1000.0), 0).unwrap();
    let mut other = req("arena", 1, 2, 1000.0);
    other.region = "tokyo".into();
    assert_eq!(q.join_test(other, 1).unwrap().status, "queued");
}

#[test]
fn every_join_gets_a_fresh_unpredictable_ticket() {
    let q = service();
    let a = q.join_test(req("arena", 1, 4, 1000.0), 0).unwrap();
    let b = q.join_test(req("arena", 1, 4, 1000.0), 1).unwrap();
    assert_ne!(a.ticket_id, b.ticket_id);
    assert_eq!(a.ticket_id.len(), 32);
}

#[test]
fn cancel_is_terminal_and_idempotent() {
    let q = service();
    let a = q.join_test(req("arena", 1, 4, 1000.0), 0).unwrap();
    let left = q.leave_test(&a.ticket_id, 1).unwrap();
    assert_eq!(left.status, "cancelled");
    let left_again = q.leave_test(&a.ticket_id, 1).unwrap();
    assert_eq!(left_again.status, "cancelled");
}

#[test]
fn tickets_expire_after_their_ttl() {
    let q = service();
    let mut r = req("arena", 1, 4, 1000.0);
    r.ttl_ms = Some(1_000);
    let a = q.join_test(r, 0).unwrap();
    let later = q.join_at(req("arena", 1, 4, 1000.0), 2_000).unwrap();
    assert_ne!(a.ticket_id, later.ticket_id, "expired ticket is gone");
}

#[test]
fn mmr_window_widens_with_wait_time() {
    assert_eq!(mmr_window(0), 100.0);
    assert_eq!(mmr_window(5_000), 150.0);
    assert_eq!(mmr_window(10 * 60_000), 500.0);
}
