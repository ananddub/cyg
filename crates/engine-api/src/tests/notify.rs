use std::time::Duration;

use uuid::Uuid;

use crate::infra::notify::{
    CHANNEL_EMAIL, CHANNEL_PUSH, KIND_ALL, PROVIDER_APNS, PROVIDER_FCM, PROVIDER_SMTP, channel_of,
    template, token,
};
use crate::infra::seats::b64url;
use crate::repositories::models::NotificationPreference;
use crate::services::notify::outbox::permitted;
use crate::services::notify::preferences::preference_row;
use crate::services::notify::worker::{retry_delay, stalled_before};

fn rows(pairs: &[(&str, &str, bool)]) -> Vec<NotificationPreference> {
    pairs
        .iter()
        .map(|(channel, kind, enabled)| preference_row(Uuid::nil(), channel, kind, *enabled))
        .collect()
}

/// The absence of a row is a yes: switching the surface on must not need one
/// write per account per kind.
#[test]
fn a_kind_nobody_has_spoken_about_is_still_being_sent() {
    assert!(permitted(&[], CHANNEL_PUSH, "friend_request"));
    assert!(permitted(
        &rows(&[(CHANNEL_PUSH, "group_add", false)]),
        CHANNEL_PUSH,
        "friend_request"
    ));
}

#[test]
fn the_account_can_switch_one_kind_off_and_the_rest_still_arrive() {
    let held = rows(&[
        (CHANNEL_PUSH, "friend_request", false),
        (CHANNEL_PUSH, KIND_ALL, true),
    ]);
    assert!(!permitted(&held, CHANNEL_PUSH, "friend_request"));
    assert!(permitted(&held, CHANNEL_PUSH, "group_add"));
}

/// A kind's own row outranks the channel's wildcard in both directions, so an
/// account that muted its mail can still be told when its account is restricted.
#[test]
fn one_kind_overrules_the_whole_channel() {
    let held = rows(&[
        (CHANNEL_EMAIL, KIND_ALL, false),
        (CHANNEL_EMAIL, "user_banned", true),
    ]);
    assert!(!permitted(&held, CHANNEL_EMAIL, "dm_request"));
    assert!(permitted(&held, CHANNEL_EMAIL, "user_banned"));
}

/// A channel is not the other channel's business.
#[test]
fn switching_off_push_leaves_mail_alone() {
    let held = rows(&[(CHANNEL_PUSH, KIND_ALL, false)]);
    assert!(!permitted(&held, CHANNEL_PUSH, "dm_request"));
    assert!(permitted(&held, CHANNEL_EMAIL, "dm_request"));
}

#[test]
fn each_retry_is_owed_longer_than_the_one_before_it() {
    for attempts in 1..8 {
        assert!(
            retry_delay(attempts + 1) > retry_delay(attempts),
            "attempt {} is not owed a longer wait than {}",
            attempts + 1,
            attempts
        );
    }
}

/// The step caps, so a relay that is down for an hour has not pushed a message a
/// day into the future by the time it answers.
#[test]
fn a_backoff_never_grows_past_the_cap() {
    for attempts in 1..24 {
        let wait = retry_delay(attempts);
        assert!(
            wait <= Duration::from_millis(1_400_000),
            "attempt {attempts} waits {} milliseconds",
            wait.as_millis()
        );
    }
}

#[test]
fn a_stalled_row_is_one_that_missed_two_timeouts() {
    let now = chrono::Utc::now();
    assert_eq!(
        stalled_before(now, 5_000),
        now - chrono::Duration::milliseconds(10_000)
    );
}

#[test]
fn a_known_kind_is_written_as_its_own_sentence() {
    let body = template::email("friend_request", "Ada wants to add you.", "");
    assert!(body.contains("Someone wants to add you."), "{body}");
    assert!(body.contains("Ada wants to add you."), "{body}");
    assert!(!body.contains("Stop these mails"), "{body}");
}

/// An unknown kind is still a fact this recipient asked to hear about, so it is
/// sent as written rather than dropped.
#[test]
fn an_unknown_kind_is_sent_the_sentence_and_the_link() {
    let body = template::email(
        "season_started",
        "The season opened.",
        "https://example.test/x",
    );
    assert!(body.contains("The season opened."), "{body}");
    assert!(
        body.ends_with("Stop these mails: https://example.test/x\n"),
        "{body}"
    );
}

#[test]
fn a_provider_names_the_channel_it_carries() {
    assert_eq!(channel_of(PROVIDER_APNS), Some(CHANNEL_PUSH));
    assert_eq!(channel_of(PROVIDER_FCM), Some(CHANNEL_PUSH));
    assert_eq!(channel_of(PROVIDER_SMTP), Some(CHANNEL_EMAIL));
    assert_eq!(channel_of("gcm"), None);
}

#[test]
fn one_row_per_account_channel_and_kind() {
    let uid = Uuid::new_v4();
    let row = preference_row(uid, CHANNEL_EMAIL, "dm_request", false);
    assert_eq!(row.id, format!("{uid}:email:dm_request"));
    assert!(!row.enabled);
}

const SECRET: &str = "a-signing-secret-long-enough-to-be-real";
const RETIRED: &str = "a-retired-secret-long-enough-to-be-real";

fn target(user_id: Uuid) -> token::Unsubscribe {
    token::Unsubscribe {
        user_id,
        channel: CHANNEL_EMAIL.to_owned(),
        kind: KIND_ALL.to_owned(),
    }
}

fn split(signed: &str) -> (&str, &str) {
    signed.split_once('.').expect("a payload and a signature")
}

#[test]
fn a_link_says_who_it_was_sent_to() {
    let uid = Uuid::new_v4();
    let signed = token::mint(SECRET, &target(uid), 3_600);
    assert_eq!(token::verify(SECRET, "", &signed), Some(target(uid)));
}

/// The links already sitting in inboxes were signed with the key this process
/// just retired, so a rotation keeps honouring them until they expire.
#[test]
fn a_rotated_key_still_opens_the_links_it_already_sent() {
    let uid = Uuid::new_v4();
    let signed = token::mint(RETIRED, &target(uid), 3_600);
    assert_eq!(token::verify(SECRET, RETIRED, &signed), Some(target(uid)));
    assert_eq!(token::verify(SECRET, "", &signed), None);
}

/// The payload travels in the clear, so rewriting whose account it names is one
/// edit away — and that edit has to break the signature over it.
#[test]
fn a_forged_link_opens_nothing() {
    let uid = Uuid::new_v4();
    let signed = token::mint(SECRET, &target(uid), 3_600);
    let (payload, mac) = split(&signed);
    let other = Uuid::new_v4();
    let borrowed = format!(
        "{}.{mac}",
        b64url(format!("{other}:email:*:2000000000").as_bytes())
    );
    assert_eq!(token::verify(SECRET, RETIRED, &borrowed), None);
    assert_eq!(
        token::verify(
            SECRET,
            RETIRED,
            &format!("{payload}.{}", b64url(&[0u8; 32]))
        ),
        None
    );
    assert_eq!(
        token::verify("another-secret-entirely", RETIRED, &signed),
        None
    );
    assert_eq!(token::verify(SECRET, RETIRED, &format!("{signed}x")), None);
    assert_eq!(token::verify(SECRET, RETIRED, "no-dot-at-all"), None);
}

#[test]
fn an_expired_link_is_refused() {
    let uid = Uuid::new_v4();
    let stale = token::mint(SECRET, &target(uid), -1);
    assert_eq!(token::verify(SECRET, RETIRED, &stale), None);
}

/// A secret the operator never set is not a key, and must not verify anything —
/// including a link minted against that same nothing.
#[test]
fn an_empty_secret_is_not_a_key() {
    let uid = Uuid::new_v4();
    let signed = token::mint("", &target(uid), 3_600);
    assert_eq!(token::verify("", "", &signed), None);
}
