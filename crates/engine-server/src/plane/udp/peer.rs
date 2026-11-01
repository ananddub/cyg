use std::time::Instant;

use crossbeam_channel::Receiver;
use engine_bus::PeerId;

use crate::plane::reliable::ReliableLane;
use crate::plane::udp::bucket::TokenBucket;

pub(super) struct Peer {
    pub(super) session_id: PeerId,
    pub(super) player_id: i64,
    pub(super) bound: bool,
    pub(super) last_seen: Instant,
    pub(super) bucket: TokenBucket,
    pub(super) pending_reply: Option<Receiver<i64>>,
    pub(super) room: String,
    pub(super) reliable: ReliableLane,
}

impl Peer {
    pub(super) fn new(
        session_id: PeerId,
        last_seen: Instant,
        room: String,
        pending_reply: Receiver<i64>,
    ) -> Peer {
        Peer {
            session_id,
            player_id: 0,
            bound: false,
            last_seen,
            bucket: TokenBucket::new_at(last_seen),
            pending_reply: Some(pending_reply),
            room,
            reliable: ReliableLane::new(),
        }
    }
}
