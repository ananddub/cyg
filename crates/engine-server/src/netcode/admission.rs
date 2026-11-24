//! Shared admission: transport is a delivery pipe, netcode is the game
//! protocol. Every lane admits through the same seat/token verification.

use std::fmt;

use crate::plane::seats::SeatError;
use crate::plane::SeatRegistry;

/// Netcode lane identity. Used for admission telemetry and policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lane {
    Ws,
    UdpHost,
    UdpPlane,
    WebRtc,
    Quic,
}

impl Lane {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ws => "ws",
            Self::UdpHost => "udp-host",
            Self::UdpPlane => "udp-plane",
            Self::WebRtc => "webrtc",
            Self::Quic => "quic",
        }
    }
}

impl fmt::Display for Lane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical wire deny reasons. Same protocol over every transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DenyReason {
    BadToken = 1,
    RoomUnknown = 2,
    RoomFull = 3,
    Expired = 4,
    RoomMismatch = 5,
    Replayed = 6,
}

impl DenyReason {
    pub const fn code(self) -> u8 {
        self as u8
    }

    pub const fn from_seat_error(err: SeatError) -> Self {
        match err {
            SeatError::Bad => Self::BadToken,
            SeatError::Expired => Self::Expired,
            SeatError::RoomMismatch => Self::RoomMismatch,
            SeatError::Replayed => Self::Replayed,
        }
    }
}

/// Admitted session identity returned by shared admission.
#[derive(Clone, Debug)]
pub struct AdmittedSession {
    pub lane: Lane,
    pub room: String,
}

/// Shared admission over any transport lane.
///
/// Verifies the seat token (signature, expiry, optional room match, replay
/// deny) and returns the admitted room or a canonical deny reason.
pub fn admit(
    lane: Lane,
    seats: &SeatRegistry,
    token: &str,
    expected_room: Option<&str>,
) -> Result<AdmittedSession, DenyReason> {
    match seats.verify(token, expected_room) {
        Ok(grant) => {
            tracing::debug!(lane = lane.as_str(), room = %grant.room, "seat admitted");
            Ok(AdmittedSession {
                lane,
                room: grant.room,
            })
        }
        Err(err) => {
            let reason = DenyReason::from_seat_error(err);
            tracing::debug!(
                lane = lane.as_str(),
                ?err,
                reason = reason.code(),
                "seat denied"
            );
            Err(reason)
        }
    }
}

/// Parse a raw JOIN frame and admit it. Convenience for UDP lanes.
pub fn admit_join_frame(
    lane: Lane,
    seats: &SeatRegistry,
    frame: &[u8],
    expected_room: Option<&str>,
) -> Result<AdmittedSession, DenyReason> {
    let Some(token) = crate::plane::join_token(frame) else {
        return Err(DenyReason::BadToken);
    };
    admit(lane, seats, token, expected_room)
}
