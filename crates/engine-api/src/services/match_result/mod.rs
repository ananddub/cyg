use auto_di::singleton;

pub mod badges;
pub mod record;

/// How many players a reported result may name.
const MAX_PARTICIPANTS: usize = 64;
/// How many counters one result may raise in total, so a single report cannot
/// write an unbounded transaction.
const MAX_COUNTERS: usize = 128;
/// The score a board can hold. Kept well inside an `i64` so a board that
/// increments cannot overflow into a negative rank.
const MAX_SCORE: i64 = 1_000_000_000_000_000;
/// The most one counter may be raised by one result.
const MAX_INCREMENT: i64 = 1_000_000;

pub struct MatchResultService;

#[singleton]
impl MatchResultService {
    fn new() -> Self {
        Self
    }
}
