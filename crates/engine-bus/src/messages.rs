use crossbeam_channel::Sender;

pub type PeerId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    NativeUdp,
    Browser,
}

/// `Unreliable` is newest-wins and droppable; `Reliable` is never dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Unreliable,
    Reliable,
}

/// One client input traveling planes → sim.
#[derive(Clone, Debug)]
pub struct InputMsg {
    pub player_id: i64,
    /// Client input sequence, echoed as `acked_seq` in the delta tail.
    pub seq: u16,
    /// Handed verbatim to `wasm.on_input`; planes never interpret it.
    pub payload: [u8; 20],
    /// `mono_ns()` at kernel read time; the sim subtracts it on drain.
    pub arrival_ns: u64,
    pub via: Via,
}

/// `bytes` is exactly the on-the-wire frame built by the sim's channel layer.
#[derive(Clone, Debug)]
pub struct OutFrame {
    pub peer_id: PeerId,
    pub kind: FrameKind,
    pub bytes: Vec<u8>,
}

/// Everything the sim accepts from the other planes.
#[derive(Debug)]
pub enum SimCommand {
    /// The sim binds the peer, assigns the player id and answers `reply`.
    BindPeer {
        peer_id: PeerId,
        room: String,
        reply: Sender<i64>,
    },
    DropPeer {
        peer_id: PeerId,
    },
    /// `target: None` broadcasts to the room.
    Event {
        room: String,
        target: Option<PeerId>,
        bytes: Vec<u8>,
    },
}
