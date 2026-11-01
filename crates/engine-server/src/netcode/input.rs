pub const MAX_INPUT_AXIS: f32 = 10_000.0;

pub const MAX_SEQ_AHEAD: u16 = 512;

pub const OP_INPUT_BATCH: u8 = 0x26;

pub const MAX_BATCHED_INPUTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRejection {
    NotSeated,
    Malformed,
}

impl InputRejection {
    pub fn label(self) -> &'static str {
        match self {
            InputRejection::NotSeated => "not-seated",
            InputRejection::Malformed => "malformed",
        }
    }
}

pub fn validate_input_payload(payload: &[u8; 20]) -> bool {
    [4usize, 8, 12, 16].iter().all(|off| {
        let lane = f32::from_le_bytes(payload[*off..*off + 4].try_into().unwrap());
        lane.is_finite() && lane.abs() <= MAX_INPUT_AXIS
    })
}

pub fn batch_sample_count(data: &[u8]) -> Option<usize> {
    if data.len() <= 3 || data[0] != OP_INPUT_BATCH {
        return None;
    }
    let samples = (data.len() - 3) / 22;
    if samples == 0 || data.len() != 3 + samples * 22 {
        return None;
    }
    if samples != data[1] as usize || samples > MAX_BATCHED_INPUTS {
        return None;
    }
    Some(samples)
}
