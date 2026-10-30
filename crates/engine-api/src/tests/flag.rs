use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::services::flag::assign::{CONTROL, TREATMENT, bucket, eligible, place, within_window};

fn arms(pairs: &[(&str, u32)]) -> Vec<(String, u32)> {
    pairs
        .iter()
        .map(|(name, weight)| ((*name).to_owned(), *weight))
        .collect()
}

fn population() -> Vec<Uuid> {
    (1..=1000u128).map(Uuid::from_u128).collect()
}

/// The rollout is a share of the population, not a queue: a tenth of the
/// accounts in or out is the most a hash of this size may be wrong by.
fn within(count: usize, expected: usize) {
    assert!(
        count.abs_diff(expected) <= 100,
        "{count} is not within ten percent of {expected}"
    );
}

/// A fixed instant, so a window boundary is a fact and not a race with a clock.
fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("a fixed timestamp")
        .with_timezone(&Utc)
}

#[test]
fn the_same_account_lands_in_the_same_arm_every_time() {
    let split = arms(&[("control", 60), ("treatment", 40)]);
    for uid in population() {
        let once = place("ab-test", "salt", uid, true, 100, &split, None);
        let twice = place("ab-test", "salt", uid, true, 100, &split, None);
        assert_eq!(once, twice, "{uid} was told two different arms");
        assert!(
            bucket("ab-test", "salt", uid) < 100,
            "{uid}: a bucket is a percentage"
        );
    }
}

#[test]
fn the_split_follows_the_weights() {
    let split = arms(&[("control", 60), ("treatment", 40)]);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for uid in population() {
        *counts
            .entry(place("ab-test", "salt", uid, true, 100, &split, None).variant)
            .or_default() += 1;
    }
    within(counts["control"], 600);
    within(counts["treatment"], 400);
}

#[test]
fn a_rollout_of_sixty_leaves_the_rest_in_the_control() {
    let mut in_rollout = 0;
    for uid in population() {
        let told = place("new-ui", "salt", uid, true, 60, &[], None);
        assert_eq!(told.on, told.variant == TREATMENT);
        assert_eq!(eligible("new-ui", "salt", uid, 60), told.on);
        if told.on {
            in_rollout += 1;
        }
    }
    within(in_rollout, 600);
    // Nothing is in the rollout at all, and everybody is: the two ends of the
    // knob, both decided without reading a single account.
    assert!(
        population()
            .iter()
            .all(|uid| !eligible("new-ui", "salt", *uid, 0))
    );
    assert!(
        population()
            .iter()
            .all(|uid| eligible("new-ui", "salt", *uid, 100))
    );
    let split = arms(&[("control", 50), ("treatment", 50)]);
    for uid in population() {
        assert_eq!(
            place("new-ui", "salt", uid, true, 0, &split, None).variant,
            CONTROL
        );
    }
}

/// A flag with no arms of its own is a switch, and the two names it answers with
/// are the two an integrator can build against.
#[test]
fn a_toggle_without_arms_answers_control_or_treatment() {
    let off = place("switch", "salt", Uuid::from_u128(7), true, 0, &[], None);
    assert_eq!((off.variant.as_str(), off.on), (CONTROL, false));
    let on = place("switch", "salt", Uuid::from_u128(7), true, 100, &[], None);
    assert_eq!((on.variant.as_str(), on.on), (TREATMENT, true));
}

#[test]
fn a_pinned_account_ignores_the_hash() {
    let split = arms(&[("control", 50), ("treatment", 50)]);
    for uid in population() {
        let kept_out = place("ab-test", "salt", uid, true, 0, &split, Some("treatment"));
        assert_eq!(
            (kept_out.variant.as_str(), kept_out.on),
            ("treatment", true)
        );
        let kept_in = place("ab-test", "salt", uid, true, 100, &split, Some("control"));
        assert_eq!((kept_in.variant.as_str(), kept_in.on), ("control", false));
    }
}

/// A pin written against an arm the flag does not define is a mistake, and the
/// hash still answers: nobody is told about a test that does not exist.
#[test]
fn a_pin_that_names_no_arm_is_ignored() {
    let split = arms(&[("control", 50), ("treatment", 50)]);
    let told = place(
        "ab-test",
        "salt",
        Uuid::from_u128(7),
        true,
        0,
        &split,
        Some("gone"),
    );
    assert_eq!((told.variant.as_str(), told.on), ("control", false));
    let free = place(
        "ab-test",
        "salt",
        Uuid::from_u128(7),
        true,
        0,
        &[],
        Some("gone"),
    );
    assert_eq!((free.variant.as_str(), free.on), ("gone", true));
}

/// The kill switch wins over everything else defined against the flag, so the
/// one state that guarantees nobody sees the new code is the one that needs no
/// other row to be in the right shape.
#[test]
fn a_stopped_flag_is_the_control_for_everybody() {
    let split = arms(&[("control", 50), ("treatment", 50)]);
    for uid in population() {
        let told = place(
            "ab-test",
            "salt",
            uid,
            false,
            100,
            &split,
            Some("treatment"),
        );
        assert_eq!((told.variant.as_str(), told.on), ("control", false));
    }
}

/// Reshuffling an experiment is a new salt, not a new key: the accounts a test
/// has already measured stay put until somebody asks for a fresh draw.
#[test]
fn a_new_salt_moves_accounts_and_a_reused_one_does_not() {
    let split = arms(&[("control", 50), ("treatment", 50)]);
    let before = population()
        .iter()
        .map(|uid| place("ab-test", "first", *uid, true, 100, &split, None).variant)
        .collect::<Vec<_>>();
    let after = population()
        .iter()
        .map(|uid| place("ab-test", "second", *uid, true, 100, &split, None).variant)
        .collect::<Vec<_>>();
    let moved = before
        .iter()
        .zip(&after)
        .filter(|(was, now)| was != now)
        .count();
    assert!(moved > 100, "{moved} of 1000 accounts moved arms");
    assert_eq!(
        before,
        population()
            .iter()
            .map(|uid| place("ab-test", "first", *uid, true, 100, &split, None).variant)
            .collect::<Vec<_>>(),
        "the same salt drew the same arms"
    );
}

/// Arms that take nobody are allowed: a flag can be defined with a treatment on
/// zero weight and opened later without anyone's assignment changing.
#[test]
fn an_arm_of_zero_weight_takes_nobody() {
    let split = arms(&[("control", 100), ("treatment", 0)]);
    for uid in population() {
        assert_eq!(
            place("ab-test", "salt", uid, true, 100, &split, None).variant,
            "control"
        );
    }
}

/// A scheduled experiment is a door on the same flag, not a second flag: the
/// window is half-open, so it opens exactly at `from` and closes exactly at
/// `to`, and the accounts it drew stay put for when it opens again.
#[test]
fn a_window_is_open_from_the_minute_it_opens_until_the_minute_it_closes() {
    let opens = at("2026-06-01T12:00:00Z");
    let closes = opens + Duration::hours(1);
    assert!(!within_window(
        Some(opens),
        Some(closes),
        opens - Duration::minutes(1)
    ));
    assert!(within_window(Some(opens), Some(closes), opens));
    assert!(within_window(
        Some(opens),
        Some(closes),
        closes - Duration::minutes(1)
    ));
    assert!(!within_window(Some(opens), Some(closes), closes));
    assert!(
        within_window(Some(opens), None, closes),
        "a window with no close stays open"
    );
    assert!(
        within_window(None, Some(closes), opens),
        "a window with no open is already open"
    );
    assert!(within_window(None, None, opens), "no window is always open");
}
