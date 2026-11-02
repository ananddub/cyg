pub type PeerId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    NativeUdp,
    Browser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Unreliable,
    Reliable,
}

#[derive(Clone, Debug)]
pub struct InputMsg {
    pub player_id: i64,

    pub seq: u16,

    pub payload: [u8; 20],

    pub arrival_ns: u64,
    pub via: Via,
}

#[derive(Clone, Debug)]
pub struct OutFrame {
    pub peer_id: PeerId,
    pub kind: FrameKind,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct SimEvent {
    pub room: String,

    pub tick: i64,

    pub seq: u64,

    pub target: i64,
    pub event: String,
    pub payload: Vec<u8>,
}
