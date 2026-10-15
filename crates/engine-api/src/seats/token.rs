/// Live seats a single registry will hold. Reaching this means abandoned page
/// loads are outrunning the expiry sweep.
pub const MAX_OUTSTANDING_SEATS: usize = 50_000;

/// How far `mint` may push a seat's expiry to keep tokens unique. Past this the
/// mint is refused instead.
pub const MAX_TTL_DRIFT_SECS: u64 = 4096;

/// A seat this plane handed out and is still willing to honour.
///
/// `Eq` because queued offers are compared to the seat that minted them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    pub id: String,
    pub room: String,
    pub token: String,
    /// Unix seconds after which the token is refused.
    pub expires_at: u64,
}
