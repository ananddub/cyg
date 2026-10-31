use crate::infra::seats::Seat;

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
