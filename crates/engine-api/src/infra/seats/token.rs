pub const MAX_OUTSTANDING_SEATS: usize = 50_000;

pub const MAX_TTL_DRIFT_SECS: u64 = 4096;

// `Seat` itself now lives in `engine-signaling` (the signaling frames carry
// it); re-exported from the seats module so this file's registry keeps its
// single seat type.
