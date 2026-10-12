//! Minimal HTTP/3 layer: frame codec, SETTINGS negotiation and capsules.
//!
//! Deliberately tiny: enough of RFC 9114 to host WebTransport sessions
//! (control stream + request streams with HEADERS/DATA), no push, no
//! priority, no dynamic QPACK table (we advertise capacity 0).

use crate::varint::{self, encode_into};

pub mod frame_type {
    pub const DATA: u64 = 0x00;
    pub const HEADERS: u64 = 0x01;
    pub const SETTINGS: u64 = 0x04;
}

pub mod setting {
    pub const QPACK_MAX_TABLE_CAPACITY: u64 = 0x01;
    pub const MAX_FIELD_SECTION_SIZE: u64 = 0x03;
    pub const QPACK_BLOCKED_STREAMS: u64 = 0x06;
    pub const ENABLE_CONNECT_PROTOCOL: u64 = 0x08;
    pub const ENABLE_WEBTRANSPORT: u64 = 0x2b;
    pub const H3_DATAGRAM: u64 = 0x33;
}

pub mod stream_type {
    /// h3 control stream (uni, both peers).
    pub const CONTROL: u64 = 0x00;
    /// h3 push stream (uni, server).
    pub const PUSH: u64 = 0x01;
    /// QPACK encoder injections (uni, client).
    pub const QPACK_ENCODER: u64 = 0x02;
    /// QPACK decoder updates (uni, server).
    pub const QPACK_DECODER: u64 = 0x03;
    /// WebTransport bidi stream prefix (both directions).
    pub const WT_BIDI: u64 = 0x41;
    /// WebTransport uni stream, client-initiated.
    pub const WT_UNI_CLIENT: u64 = 0x51;
    /// WebTransport uni stream, server-initiated.
    pub const WT_UNI_SERVER: u64 = 0x54;
}

pub mod capsule_type {
    pub const CLOSE_SESSION: u64 = 0x2843d064;
}

/// QUIC error code used when resetting WebTransport streams of a session.
pub const WT_APPLICATION_ERROR: u64 = 0x5388e507;

/// Encode an h3 frame: `type(varint) length(varint) payload`.
pub fn build_frame(kind: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + payload.len());
    encode_into(kind, &mut out);
    encode_into(payload.len() as u64, &mut out);
    out.extend_from_slice(payload);
    out
}

/// Frame prefixed with its encoded length, for use inside stream data
/// (capsule framing per RFC 9297 on request streams).
pub fn build_length_prefixed(frame: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + frame.len());
    encode_into(frame.len() as u64, &mut out);
    out.extend_from_slice(frame);
    out
}

/// Wrap a capsule in the varint length prefix required when sending it as
/// request-stream DATA payload.
pub fn frame_capsule(capsule: &[u8]) -> Vec<u8> {
    build_frame(frame_type::DATA, &build_length_prefixed(capsule))
}

#[derive(Debug)]
pub enum FrameParse {
    Incomplete,
}

/// Try to parse one complete frame from the front of `buf`; `consumed` bytes
/// on success.
pub fn try_parse_frame(buf: &[u8]) -> Result<(u64, Vec<u8>, usize), FrameParse> {
    let (kind, n1) = varint::decode(buf).ok_or(FrameParse::Incomplete)?;
    let rest = &buf[n1..];
    let (len, n2) = varint::decode(rest).ok_or(FrameParse::Incomplete)?;
    if len > 64 * 1024 * 1024 {
        return Err(FrameParse::Incomplete); // treat absurd sizes as stall; upper layer caps buffer
    }
    let body = &rest[n2..];
    if body.len() < len as usize {
        return Err(FrameParse::Incomplete);
    }
    Ok((kind, body[..len as usize].to_vec(), n1 + n2 + len as usize))
}

/// Capsule: `type(varint) length(varint) value`, length-prefixed on streams.
/// Parses one length-prefixed capsule; returns `(type, value, consumed)`.
pub fn try_parse_capsule(buf: &[u8]) -> Option<(u64, Vec<u8>, usize)> {
    let (outer_len, n1) = varint::decode(buf)?;
    let body = buf.get(n1..n1.checked_add(outer_len as usize)?)?;
    let (kind, n2) = varint::decode(body)?;
    let (vlen, n3) = varint::decode(body.get(n2..)?)?;
    let start = n2.checked_add(n3)?;
    let value = body.get(start..start.checked_add(vlen as usize)?)?.to_vec();
    Some((kind, value, n1 + outer_len as usize))
}

/// Build a CLOSE_WEBTRANSPORT_SESSION capsule (unframed).
pub fn build_close_capsule(error_code: u64, reason: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    encode_into(error_code, &mut payload);
    encode_into(reason.len() as u64, &mut payload);
    payload.extend_from_slice(reason.as_bytes());

    let mut capsule = Vec::new();
    encode_into(capsule_type::CLOSE_SESSION, &mut capsule);
    encode_into(payload.len() as u64, &mut capsule);
    capsule.extend_from_slice(&payload);
    capsule
}

/// Decode a CLOSE capsule value; errors when it is not one.
pub fn parse_close_capsule_value(value: &[u8]) -> Option<(u64, String)> {
    let mut cur = value;
    let code = varint::take(&mut cur)?;
    let rlen = varint::take(&mut cur)? as usize;
    if cur.len() < rlen {
        return None;
    }
    let reason = String::from_utf8_lossy(&cur[..rlen]).into_owned();
    Some((code, reason))
}

/// Peer SETTINGS as we care about them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerSettings {
    pub enable_connect_protocol: bool,
    pub enable_webtransport: bool,
    pub h3_datagram: bool,
    pub qpack_max_table_capacity: u64,
    pub max_field_section_size: u64,
}

/// Our SETTINGS payload: full WT support, zero QPACK dynamic table so the
/// stateless codec is spec-legal for any compliant client.
pub fn server_settings_payload() -> Vec<u8> {
    let mut out = Vec::new();
    let mut kv = |k: u64, v: u64| {
        encode_into(k, &mut out);
        encode_into(v, &mut out);
    };
    kv(setting::QPACK_MAX_TABLE_CAPACITY, 0);
    kv(setting::QPACK_BLOCKED_STREAMS, 0);
    kv(setting::MAX_FIELD_SECTION_SIZE, 65536);
    kv(setting::ENABLE_CONNECT_PROTOCOL, 1);
    kv(setting::ENABLE_WEBTRANSPORT, 1);
    kv(setting::H3_DATAGRAM, 1);
    out
}

pub fn parse_settings(payload: &[u8]) -> PeerSettings {
    let mut s = PeerSettings::default();
    let mut cur = payload;
    while let Some((k, n1)) = varint::decode(cur) {
        let rest = &cur[n1..];
        let Some((v, n2)) = varint::decode(rest) else {
            break;
        };
        match k {
            setting::QPACK_MAX_TABLE_CAPACITY => s.qpack_max_table_capacity = v,
            setting::MAX_FIELD_SECTION_SIZE => s.max_field_section_size = v,
            setting::ENABLE_CONNECT_PROTOCOL => s.enable_connect_protocol = v == 1,
            setting::ENABLE_WEBTRANSPORT => s.enable_webtransport = v == 1,
            setting::H3_DATAGRAM => s.h3_datagram = v == 1,
            _ => {}
        }
        cur = &rest[n2..];
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let f = build_frame(frame_type::HEADERS, b"hello");
        let (kind, payload, consumed) = try_parse_frame(&f).unwrap();
        assert_eq!(kind, frame_type::HEADERS);
        assert_eq!(payload, b"hello");
        assert_eq!(consumed, f.len());
    }

    #[test]
    fn frame_split_across_feeds_is_incomplete() {
        let f = build_frame(frame_type::DATA, &vec![7u8; 100]);
        match try_parse_frame(&f[..30]) {
            Err(FrameParse::Incomplete) => {}
            _ => panic!("should be incomplete"),
        }
    }

    #[test]
    fn settings_roundtrip() {
        let payload = server_settings_payload();
        let s = parse_settings(&payload);
        assert!(s.enable_connect_protocol);
        assert!(s.enable_webtransport);
        assert!(s.h3_datagram);
        assert_eq!(s.qpack_max_table_capacity, 0);
    }

    #[test]
    fn close_capsule_roundtrip() {
        let c = build_close_capsule(WT_APPLICATION_ERROR, "server shutting down");
        let framed = build_length_prefixed(&c);
        let (kind, value, consumed) = try_parse_capsule(&framed).unwrap();
        assert_eq!(kind, capsule_type::CLOSE_SESSION);
        assert_eq!(consumed, framed.len());
        let (code, reason) = parse_close_capsule_value(&value).unwrap();
        assert_eq!(code, WT_APPLICATION_ERROR);
        assert_eq!(reason, "server shutting down");
    }
}
