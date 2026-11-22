use engine_schema::InputDomainMap;

use crate::rooms::SessionSeqs;

pub const MAX_INPUT_AXIS: f32 = 10_000.0;

pub const MAX_SEQ_AHEAD: u16 = 512;

pub const OP_INPUT_BATCH: u8 = 0x26;

pub const MAX_BATCHED_INPUTS: usize = 64;

/// Standard 20-byte input payload layout (the bytes after the 3-byte mesh
/// header; `InputFrame::decode` reads the same lanes from the wire):
/// `[flags u32 LE][aim_yaw f32][aim_pitch f32][move_x f32][move_z f32]`.
pub mod input_lane {
    pub const FLAGS: usize = 0;
    pub const AIM_YAW: usize = 4;
    pub const AIM_PITCH: usize = 8;
    pub const MOVE_X: usize = 12;
    pub const MOVE_Z: usize = 16;
}

/// Default domains of the standard frame's float lanes — the host-side
/// equivalent of Colyseus' `defineInput({ sanitize })` map form. The mesh
/// protocol quantizes angles to `[-π, π]` (see the client's
/// `quantizeAngle`), so aim lanes get that window; movement axes are the
/// usual `[-1, 1]`. A room's `Input` schema can override any entry through
/// `GameRoom::set_input_domains` (schema-declared domains land there once
/// authoring gains a `domain` modifier); entries absent from the map fall
/// back to these.
pub const INPUT_AIM_DOMAIN: engine_schema::InputFieldDomain =
    engine_schema::InputFieldDomain::new(-core::f32::consts::PI, core::f32::consts::PI);

pub const INPUT_MOVE_DOMAIN: engine_schema::InputFieldDomain =
    engine_schema::InputFieldDomain::new(-1.0, 1.0);

/// The standard frame's domains keyed by schema field name.
pub fn standard_input_domains() -> InputDomainMap {
    let mut map = InputDomainMap::new();
    for (name, dom) in [
        ("aim_yaw", INPUT_AIM_DOMAIN),
        ("aim_pitch", INPUT_AIM_DOMAIN),
        ("move_x", INPUT_MOVE_DOMAIN),
        ("move_z", INPUT_MOVE_DOMAIN),
    ] {
        map.insert(name.to_string(), dom);
    }
    map
}

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

/// Host-side, schema-driven sanitize applied IN PLACE before any consumer
/// reads the frame (Colyseus `sanitize` rule: modify, never reject — a
/// malformed value becomes a legal one). NaN lands on the clamp floor via
/// `InputFieldDomain::clamp`, so the guest can never observe a poisoned
/// float; honest clients are unaffected (in-range values are identity),
/// which keeps client prediction aligned with the server's view.
pub fn sanitize_input_payload(payload: &mut [u8; 20], domains: &InputDomainMap) {
    for (lane, name) in [
        (input_lane::AIM_YAW, "aim_yaw"),
        (input_lane::AIM_PITCH, "aim_pitch"),
        (input_lane::MOVE_X, "move_x"),
        (input_lane::MOVE_Z, "move_z"),
    ] {
        let dom = domains
            .get(name)
            .copied()
            .unwrap_or(if name.starts_with("move") {
                INPUT_MOVE_DOMAIN
            } else {
                INPUT_AIM_DOMAIN
            });
        let v = f32::from_le_bytes(payload[lane..lane + 4].try_into().unwrap());
        payload[lane..lane + 4].copy_from_slice(&dom.clamp(v).to_le_bytes());
    }
}

/// True for a schema-lane input app frame: `[0x05][7]["__input"][seq u16
/// LE][frame...]` (the reserved lane the generated guest glue decodes).
/// The generic on-input path uses this to record the seat's last frame for
/// idle synthesis and to peel the RTT stamp trailer.
pub fn is_input_app_frame(data: &[u8]) -> bool {
    data.len() >= 4
        && data[0] == 0x05
        && data[1] as usize == b"__input".len()
        && &data[2..9] == b"__input"
}

/// Bump the schema lane's own seq u16 (offset 9, right after the
/// `"__input"` tag) by one, wrapping. The generated guest glue drops a
/// frame whose seq does not advance, so a re-delivered idle frame must —
/// while the host's reconcile seq (`room.seqs.input`, the delta-tail ack)
/// stays on the last REAL input, per the hold-last contract.
/// Reserved schema input batch tag. The client replays the newest unacked
/// frames in one app message instead of one packet per frame.
pub const INPUT_BATCH_TAG: &[u8] = b"__input_batch";

/// Hard limits, enforced on both ends. The batch is an unreliable-lane
/// optimisation, so an oversized one is refused whole rather than fragmented.
pub const MAX_INPUT_BATCH_FRAMES: usize = 8;
pub const MAX_INPUT_BATCH_WIRE: usize = 512;

#[derive(Debug)]
pub struct InputBatch<'a> {
    pub stamp: Option<u16>,
    pub frames: Vec<(u16, &'a [u8])>,
}

impl InputBatch<'_> {
    /// Newest frame as a canonical `__input` app frame, for idle synthesis.
    pub fn latest_frame(&self) -> Option<(u16, Vec<u8>)> {
        let &(seq, frame) = self.frames.last()?;
        let mut out = Vec::with_capacity(2 + 7 + 2 + frame.len());
        out.push(0x05);
        out.push(7);
        out.extend_from_slice(b"__input");
        out.extend_from_slice(&seq.to_le_bytes());
        out.extend_from_slice(frame);
        Some((seq, out))
    }
}

/// True for `[0x05][13]["__input_batch"][count][frames…][stamp?]`.
pub fn is_input_batch(data: &[u8]) -> bool {
    data.len() >= 2 + INPUT_BATCH_TAG.len()
        && data[0] == 0x05
        && data[1] as usize == INPUT_BATCH_TAG.len()
        && &data[2..2 + INPUT_BATCH_TAG.len()] == INPUT_BATCH_TAG
}

/// Parse and validate a schema input batch. Every frame must fit inside the
/// declared length, the count must match, and the optional RTT stamp trailer
/// is peeled off. Malformed input returns `None` so the whole packet is
/// refused (never a partially applied history).
pub fn parse_input_batch(data: &[u8]) -> Option<InputBatch<'_>> {
    const STAMP_MAGIC: [u8; 2] = [0xC7, 0x59];
    if !is_input_batch(data) || data.len() > MAX_INPUT_BATCH_WIRE {
        return None;
    }
    let header = 2 + INPUT_BATCH_TAG.len();
    let mut p = header;
    if p >= data.len() {
        return None;
    }
    let count = data[p] as usize;
    p += 1;
    if count == 0 || count > MAX_INPUT_BATCH_FRAMES {
        return None;
    }
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        if p + 4 > data.len() {
            return None;
        }
        let seq = u16::from_le_bytes([data[p], data[p + 1]]);
        let len = u16::from_le_bytes([data[p + 2], data[p + 3]]) as usize;
        p += 4;
        if len == 0 || p + len > data.len() {
            return None;
        }
        frames.push((seq, &data[p..p + len]));
        p += len;
    }
    // The only allowed tail is the optional RTT stamp.
    let stamp = if p + 4 == data.len() && data[p..p + 2] == STAMP_MAGIC {
        Some(u16::from_le_bytes([data[p + 2], data[p + 3]]))
    } else if p == data.len() {
        None
    } else {
        return None;
    };
    Some(InputBatch { stamp, frames })
}

/// Schema-lane seq for a single `__input` frame, if present.
pub fn input_app_seq(data: &[u8]) -> Option<u16> {
    if is_input_app_frame(data) && data.len() >= 11 {
        Some(u16::from_le_bytes([data[9], data[10]]))
    } else {
        None
    }
}

pub fn bump_input_app_seq(frame: &mut [u8]) {
    if frame.len() >= 11 {
        let seq = u16::from_le_bytes([frame[9], frame[10]]).wrapping_add(1);
        frame[9..11].copy_from_slice(&seq.to_le_bytes());
    }
}

/// Peel the client's 4-byte RTT stamp trailer `[0xC7 0x59][stamp u16 LE]`
/// off a schema-lane input frame, recording the stamp on the seat's seq
/// entry for the delta-tail echo. Returns the guest-ready frame (trailer
/// stripped) and, when a stamp was present, its value. Old clients that
/// send no trailer are returned untouched with `stamp = None` — the tail
/// then echoes 0 and the client simply keeps using its WS ping.
pub fn extract_input_stamp<'a>(
    seqs: &mut crate::rooms::HashMap<i64, SessionSeqs>,
    id: i64,
    data: &'a [u8],
) -> (&'a [u8], Option<u16>) {
    const STAMP_MAGIC: [u8; 2] = [0xC7, 0x59];
    if !is_input_app_frame(data) || data.len() < 6 {
        return (data, None);
    }
    let n = data.len();
    if data[n - 4..n - 2] != STAMP_MAGIC {
        return (data, None);
    }
    let stamp = u16::from_le_bytes([data[n - 2], data[n - 1]]);
    if let Some(entry) = seqs.get_mut(&id) {
        entry.stamp = stamp;
    }
    (&data[..n - 4], Some(stamp))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(yaw: f32, pitch: f32, mx: f32, mz: f32) -> [u8; 20] {
        let mut p = [0u8; 20];
        p[4..8].copy_from_slice(&yaw.to_le_bytes());
        p[8..12].copy_from_slice(&pitch.to_le_bytes());
        p[12..16].copy_from_slice(&mx.to_le_bytes());
        p[16..20].copy_from_slice(&mz.to_le_bytes());
        p
    }

    fn lane(p: &[u8; 20], off: usize) -> f32 {
        f32::from_le_bytes(p[off..off + 4].try_into().unwrap())
    }

    #[test]
    fn nan_lands_on_the_clamp_floor() {
        let mut p = payload(0.0, f32::NAN, 0.0, 0.0);
        sanitize_input_payload(&mut p, &standard_input_domains());
        assert_eq!(lane(&p, input_lane::AIM_PITCH), -core::f32::consts::PI);
    }

    #[test]
    fn an_out_of_range_move_axis_clamps_to_the_domain() {
        let mut p = payload(0.0, 0.0, 127.0, -0.5);
        sanitize_input_payload(&mut p, &standard_input_domains());
        assert_eq!(lane(&p, input_lane::MOVE_X), 1.0);
        assert_eq!(lane(&p, input_lane::MOVE_Z), -0.5);
    }

    #[test]
    fn in_range_values_are_identity() {
        let before = payload(1.25, -0.5, 0.75, -1.0);
        let mut p = before;
        sanitize_input_payload(&mut p, &standard_input_domains());
        assert_eq!(p, before, "honest clients must be untouched");
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;

    fn wire(count: u8, frames: &[(u16, &[u8])], stamp: Option<u16>) -> Vec<u8> {
        let mut out = vec![0x05, INPUT_BATCH_TAG.len() as u8];
        out.extend_from_slice(INPUT_BATCH_TAG);
        out.push(count);
        for &(seq, frame) in frames {
            out.extend_from_slice(&seq.to_le_bytes());
            out.extend_from_slice(&(frame.len() as u16).to_le_bytes());
            out.extend_from_slice(frame);
        }
        if let Some(s) = stamp {
            out.extend_from_slice(&[0xC7, 0x59]);
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }

    #[test]
    fn batch_roundtrips_and_peels_the_stamp() {
        let w = wire(2, &[(9, &[1, 2]), (10, &[3, 4, 5])], Some(0xBEEF));
        assert!(is_input_batch(&w));
        let b = parse_input_batch(&w).expect("valid");
        assert_eq!(b.stamp, Some(0xBEEF));
        assert_eq!(b.frames, vec![(9, &[1u8, 2][..]), (10, &[3u8, 4, 5][..])]);
        let (seq, latest) = b.latest_frame().unwrap();
        assert_eq!(seq, 10);
        assert_eq!(&latest[..9], b"\x05\x07__input");
        assert_eq!(&latest[9..11], &10u16.to_le_bytes());
        assert_eq!(&latest[11..], &[3, 4, 5]);
    }

    #[test]
    fn malformed_batches_are_refused_whole() {
        let good = wire(2, &[(1, &[1]), (2, &[2])], None);
        let mut bad = good.clone();
        bad[2 + INPUT_BATCH_TAG.len()] = 3;
        assert!(parse_input_batch(&bad).is_none(), "count lies");
        let mut bad = good.clone();
        bad.pop();
        assert!(parse_input_batch(&bad).is_none(), "truncated frame");
        let mut bad = good.clone();
        bad.extend_from_slice(&[9, 9]);
        assert!(parse_input_batch(&bad).is_none(), "unknown tail");
        assert!(parse_input_batch(&wire(0, &[], None)).is_none());
        assert!(parse_input_batch(&wire((MAX_INPUT_BATCH_FRAMES + 1) as u8, &[], None)).is_none());
        let big = vec![0u8; MAX_INPUT_BATCH_WIRE];
        assert!(parse_input_batch(&big).is_none(), "oversize wire");
    }
}
