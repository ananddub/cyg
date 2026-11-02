#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SignalKind {
    Join = 1,
    Offer = 2,
    Answer = 3,
    Ice = 4,
    Bye = 5,
    Config = 8,
    PeerJoined = 9,
    PeerLeft = 10,
    Signal = 11,
    Error = 12,
    Muted = 13,
}

impl SignalKind {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::Join),
            2 => Some(Self::Offer),
            3 => Some(Self::Answer),
            4 => Some(Self::Ice),
            5 => Some(Self::Bye),
            8 => Some(Self::Config),
            9 => Some(Self::PeerJoined),
            10 => Some(Self::PeerLeft),
            11 => Some(Self::Signal),
            12 => Some(Self::Error),
            13 => Some(Self::Muted),
            _ => None,
        }
    }

    #[inline]
    pub fn is_relayed(self) -> bool {
        matches!(self, Self::Offer | Self::Answer | Self::Ice | Self::Signal)
    }
}
