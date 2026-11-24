/// A granted player seat — moved here from `engine-api::infra::seats` so the
/// signaling frames can reference it without the whole API crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    pub id: String,
    pub room: String,
    pub token: String,
    pub expires_at: u64,
    pub options: String,
}
