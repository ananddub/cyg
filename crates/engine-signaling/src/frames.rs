use crate::seat::Seat;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalIn {
    Offer {
        seat: Seat,
        sdp: String,
    },
    Candidate {
        seat: Seat,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_index: Option<u32>,
    },
    Close {
        seat: Seat,
        /// Attach session that produced the close. The host ignores the close
        /// when it no longer matches the seat's current route, so a stale
        /// socket's teardown cannot kill a reconnect.
        attach_id: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalOut {
    Answer {
        sdp: String,
    },
    Candidate {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_index: Option<u32>,
    },
    IceServers {
        config: String,
    },
    Error {
        message: String,
    },
}
