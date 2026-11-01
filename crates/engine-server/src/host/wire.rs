pub(crate) fn sanitize_room_name(raw: &str) -> &str {
    let s = raw.split('?').next().unwrap_or(raw);
    s.split("sessionId=").next().unwrap_or(s)
}

pub(crate) fn strip_type_header(payload: &[u8]) -> Option<&[u8]> {
    let first = *payload.first()?;
    if (0xA0..=0xBF).contains(&first) {
        let type_len = (first & 0x1F) as usize;
        payload.get(1 + type_len..)
    } else if first == 0xD9 {
        let type_len = *payload.get(1)? as usize;
        payload.get(2 + type_len..)
    } else {
        Some(payload)
    }
}

pub(crate) fn mono_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn unix_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn rxd_summary(seen: &std::collections::HashSet<u16>) -> (u16, Vec<(u16, u16)>) {
    if seen.is_empty() {
        return (0, Vec::new());
    }
    let mut seqs: Vec<u16> = seen.iter().copied().collect();
    let base = *seqs.iter().min().expect("checked non-empty");
    seqs.sort_by_key(|s| s.wrapping_sub(base));
    let mut cum = seqs[0];
    for &s in seqs.iter().skip(1) {
        if s.wrapping_sub(cum) == 1 {
            cum = s;
        } else {
            break;
        }
    }
    let mut ranges: Vec<(u16, u16)> = Vec::new();
    for &s in seqs.iter().skip(1) {
        if engine_webrtc_protocol::is_newer(cum, s) {
            match ranges.last_mut() {
                Some(last) if s.wrapping_sub(last.1) == 1 => last.1 = s,
                _ => ranges.push((s, s)),
            }
        }
    }
    (cum, ranges)
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn rxd_summary_for_test(
    seen: &std::collections::HashSet<u16>,
) -> (u16, Vec<(u16, u16)>) {
    rxd_summary(seen)
}
