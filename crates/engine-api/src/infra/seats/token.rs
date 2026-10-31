pub const MAX_OUTSTANDING_SEATS: usize = 50_000;

pub const MAX_TTL_DRIFT_SECS: u64 = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    pub id: String,
    pub room: String,
    pub token: String,
    pub expires_at: u64,
}
