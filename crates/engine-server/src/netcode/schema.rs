pub const SCHEMA_MAGIC: &[u8; 4] = b"OXS2";
pub const SCHEMA_ENVELOPE_HEADER: usize = 8;
pub const SCHEMA_SECTION_HEADER: usize = 9;

pub struct SchemaSections<'a> {
    pub full: &'a [u8],
    pub patch: &'a [u8],
}

pub fn split_schema(buf: &[u8]) -> Option<SchemaSections<'_>> {
    if buf.len() < SCHEMA_ENVELOPE_HEADER || &buf[0..4] != SCHEMA_MAGIC {
        return None;
    }
    let full_len = u32::from_le_bytes(buf[4..8].try_into().ok()?) as usize;
    let full_start = SCHEMA_ENVELOPE_HEADER;
    let full_end = full_start.checked_add(full_len)?;
    if full_end + 4 > buf.len() {
        return None;
    }
    let patch_len = u32::from_le_bytes(buf[full_end..full_end + 4].try_into().ok()?) as usize;
    let patch_start = full_end + 4;
    let patch_end = patch_start.checked_add(patch_len)?;
    if patch_end != buf.len() {
        return None;
    }
    let full = &buf[full_start..full_end];
    let patch = &buf[patch_start..patch_end];
    if full.len() < SCHEMA_SECTION_HEADER
        || patch.len() < SCHEMA_SECTION_HEADER
        || &full[0..4] != SCHEMA_MAGIC
        || &patch[0..4] != SCHEMA_MAGIC
    {
        return None;
    }
    Some(SchemaSections { full, patch })
}

/// Tail: `[acked_seq u16][server_tick u32][client_stamp u16]`. The stamp is
/// the client's rotating RTT marker from its last schema input (Phase D RTT
/// echo, 0 when the client never stamps) — the client pairs it with its
/// send time to measure smoothed RTT/jitter without a dedicated ping lane.
pub fn wrap_delta_into(
    out: &mut Vec<u8>,
    your_id: i64,
    delta: &[u8],
    acked_seq: u16,
    server_tick: u32,
    client_stamp: u16,
) {
    out.clear();
    out.reserve(delta.len() + 24);
    out.push(0xA5);
    out.extend_from_slice(b"delta");
    out.extend_from_slice(&your_id.to_le_bytes());
    out.extend_from_slice(delta);
    out.extend_from_slice(&acked_seq.to_le_bytes());
    out.extend_from_slice(&server_tick.to_le_bytes());
    out.extend_from_slice(&client_stamp.to_le_bytes());
}

/// Patch the per-session fields of a prebuilt wrapped frame in place: the
/// 8-byte `your_id` at bytes 6..14 and the 2-byte `acked_seq` just past the
/// body. The frame must come from `wrap_delta_into` (your_id=0, acked=0,
/// stamp=0). Patching 12 bytes per session replaces a full body memcpy per
/// session.
pub fn patch_wrap_into(out: &mut [u8], your_id: i64, acked_seq: u16, client_stamp: u16) {
    out[6..14].copy_from_slice(&your_id.to_le_bytes());
    let n = out.len();
    out[n - 8..n - 6].copy_from_slice(&acked_seq.to_le_bytes());
    out[n - 2..].copy_from_slice(&client_stamp.to_le_bytes());
}
