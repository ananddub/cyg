use std::collections::BTreeMap;

use chrono::Utc;

use crate::dto::badge::BadgeRow;
use crate::repositories::models::BadgeDefinition;
use crate::services::match_result::badges::earned;

fn badge(stat: &str, threshold: i64) -> BadgeDefinition {
    BadgeDefinition {
        id: format!("milestone-{stat}"),
        game_id: None,
        title: "Milestone".to_owned(),
        detail: format!("Reach {threshold} {stat}."),
        stat: stat.to_owned(),
        threshold,
        is_active: true,
        create_time: Utc::now(),
        update_time: Utc::now(),
    }
}

fn stats(pairs: &[(&str, i64)]) -> BTreeMap<String, i64> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), *value))
        .collect()
}

/// Earning is a comparison against a counter, not a row: a badge whose counter
/// the player has never raised reads as zero, and one that has passed its
/// threshold reads earned without anything having to write it.
#[test]
fn a_badge_is_judged_by_the_counter_it_names() {
    let held = stats(&[("wins", 10)]);
    let reached: BadgeRow = earned(&badge("wins", 10), &held);
    assert_eq!((reached.value, reached.earned), (10, true));

    // The threshold is inclusive, so exactly reaching it earns the badge.
    let short = earned(&badge("wins", 11), &held);
    assert_eq!((short.value, short.earned), (10, false));

    let absent = earned(&badge("casts", 1), &held);
    assert_eq!((absent.value, absent.earned), (0, false));
}
