//! Wire types exchanged between the planes.

use crossbeam_channel::Sender;

/// Opaque per-connection identifier; room membership maps it to a player id.
pub type PeerId = String;

/// Inbound transport for an input or frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    NativeUdp,
    Browser,
}

/// Outbound lane for a framed message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// game-unreliable / raw UDP deltas: newest-wins, droppable.
    Unreliable,
    /// game-reliable / control replies (BIND, DENY, events): never dropped.
    Reliable,
}

/// One client input traveling planes → sim.
#[derive(Clone, Debug)]
pub struct InputMsg {
    /// Assigned by the sim at join time.
    pub player_id: i64,
    /// Client input sequence, echoed as `acked_seq` in the delta tail.
    pub seq: u16,
    /// Fixed 20-byte guest input blob handed verbatim to `wasm.on_input`.
    pub payload: [u8; 20],
    /// `mono_ns()` at kernel read time; the sim subtracts it on drain to get
    /// kernel→sim latency.
    pub arrival_ns: u64,
    pub via: Via,
}

/// Fully framed, transport-ready outbound message: `bytes` goes on the wire
/// exactly as built by the sim's channel layer.
#[derive(Clone, Debug)]
pub struct OutFrame {
    pub peer_id: PeerId,
    pub kind: FrameKind,
    pub bytes: Vec<u8>,
}

/// Everything the sim accepts from the other planes.
#[derive(Debug)]
pub enum SimCommand {
    /// Seat token verified; the sim binds the peer, assigns the player id and
    /// answers through `reply` (bounded(1), same-thread handoff).
    BindPeer {
        peer_id: PeerId,
        room: String,
        reply: Sender<i64>,
    },
    /// Peer disconnected or timed out; release the player slot.
    DropPeer { peer_id: PeerId },
    /// Reliable event for one peer, or for all when `target` is None.
    Event {
        room: String,
        target: Option<PeerId>,
        bytes: Vec<u8>,
    },
}
