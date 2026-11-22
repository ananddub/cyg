pub const CYG_OP_LEAVE_ROOM: u8 = 3;

pub const CYG_WIRE_JOIN_ROOM: u8 = 10;
pub const CYG_WIRE_LEAVE_ROOM: u8 = 12;
pub const CYG_WIRE_PING: u8 = 18;
pub const CYG_WIRE_SCHEMA: u8 = 0x11;

pub const CYG_WIRE_RELIABLE_MSG: u8 = 30;
pub const CYG_WIRE_RELIABLE_ACK: u8 = 31;

pub fn encode_reliable_msg(seq: u16, payload: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.push(CYG_WIRE_RELIABLE_MSG);
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(payload);
}

pub fn decode_reliable_msg(data: &[u8]) -> Option<(u16, &[u8])> {
    if data.len() < 4 || data[0] != CYG_WIRE_RELIABLE_MSG {
        return None;
    }
    let seq = u16::from_le_bytes([data[1], data[2]]);
    Some((seq, &data[3..]))
}

pub fn encode_reliable_ack(seq: u16, out: &mut Vec<u8>) {
    out.clear();
    out.push(CYG_WIRE_RELIABLE_ACK);
    out.extend_from_slice(&seq.to_le_bytes());
}

pub fn encode_reliable_ack_sack(cum: u16, ranges: &[(u16, u16)], out: &mut Vec<u8>) {
    out.clear();
    out.push(CYG_WIRE_RELIABLE_ACK);
    out.extend_from_slice(&cum.to_le_bytes());
    out.push(ranges.len().min(255) as u8);
    for &(s, e) in ranges.iter().take(255) {
        out.extend_from_slice(&s.to_le_bytes());
        out.extend_from_slice(&e.to_le_bytes());
    }
}

pub fn decode_reliable_ack(data: &[u8]) -> Option<(u16, Vec<(u16, u16)>)> {
    if data.len() < 3 || data[0] != CYG_WIRE_RELIABLE_ACK {
        return None;
    }
    let cum = u16::from_le_bytes([data[1], data[2]]);
    if data.len() == 3 {
        return Some((cum, Vec::new()));
    }
    let Some(&count) = data.get(3) else {
        return Some((cum, Vec::new()));
    };
    let mut ranges = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let base = 4 + i * 4;
        let Some(start) = data.get(base..base + 2) else {
            break;
        };
        let Some(end) = data.get(base + 2..base + 4) else {
            break;
        };
        ranges.push((
            u16::from_le_bytes([start[0], start[1]]),
            u16::from_le_bytes([end[0], end[1]]),
        ));
    }
    Some((cum, ranges))
}
