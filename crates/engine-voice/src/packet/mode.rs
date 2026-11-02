#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum VoiceChannelMode {
    #[default]
    Proximity = 0,

    Global = 1,

    TeamOnly = 2,

    Whisper = 3,
}

impl VoiceChannelMode {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Global,
            2 => Self::TeamOnly,
            3 => Self::Whisper,
            _ => Self::Proximity,
        }
    }
}

pub mod voice_flags {
    pub const STEREO: u8 = 1 << 0;
    pub const VOICE_ACTIVITY: u8 = 1 << 1;
    pub const PRIORITY_SPEAKER: u8 = 1 << 2;
}
