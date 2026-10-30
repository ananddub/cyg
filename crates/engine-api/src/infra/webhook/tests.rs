use super::*;

/// A receiver written from nothing but the documented scheme has to agree with
/// this, so the pair is fixed rather than generated at test time.
const SECRET: &str = "cyg_wh_live_0123456789abcdef";
const BODY: &str = concat!(
    r#"{"event":"quest.completed","event_id":"quest:abc","#,
    r#""occurred_at":"2026-10-30T10:00:00Z","#,
    r#""payload":{"quest_key":"win-three"}}"#
);
const TIMESTAMP: &str = "1761818400";
const HEX: &str = "123296750c15999b12710ce8da7ee3ce8f4421e046bb38bdaba92f86d76151a7";

#[test]
fn a_known_vector_signs_to_the_published_hex() {
    assert_eq!(
        signed_string(TIMESTAMP, BODY),
        format!("{TIMESTAMP}.{BODY}")
    );
    assert_eq!(signature(SECRET, TIMESTAMP, BODY), format!("v1={HEX}"));
}

#[test]
fn a_signature_within_the_window_verifies_and_one_outside_it_does_not() {
    let at = TIMESTAMP.parse::<i64>().expect("the vector is a timestamp");
    let header = signature(SECRET, TIMESTAMP, BODY);
    assert!(verifies(SECRET, TIMESTAMP, BODY, &header, at));
    // The same signature, replayed past the window, is refused.
    assert!(!verifies(
        SECRET,
        TIMESTAMP,
        BODY,
        &header,
        at + REPLAY_WINDOW + 1
    ));
    // A body that moved by one byte no longer matches.
    assert!(!verifies(
        SECRET,
        TIMESTAMP,
        &format!("{BODY} "),
        &header,
        at
    ));
    // So does one signed with the wrong secret, or with no scheme prefix.
    assert!(!verifies("other", TIMESTAMP, BODY, &header, at));
    assert!(!verifies(SECRET, TIMESTAMP, BODY, HEX, at));
    assert!(!verifies(SECRET, "not-a-time", BODY, &header, at));
}
