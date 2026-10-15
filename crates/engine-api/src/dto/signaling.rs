//! `/ws/signal` — the upgrade query.

use serde::Deserialize;

/// `GET /ws/signal?seat=<seatToken>`: the seat token a matchmake reply minted. Required.
#[derive(Deserialize)]
pub struct SignalQuery {
    pub seat: Option<String>,
}
