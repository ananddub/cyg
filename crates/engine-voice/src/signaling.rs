//! # WebRTC Signaling Wire Protocol
//!
//! Versioned, self-describing framing for peer-to-peer voice negotiation. The
//! server is a pure signalling relay: it never parses the SDP or ICE bodies, it
//! only checks the header, replaces the sender slot with its own authoritative
//! entity id, and forwards the frame to the target peer.
//!
//! Wire layout (big-endian):
//! `[1 byte MAGIC 'W'][1 byte version][1 byte kind][4 byte from][4 byte to][N bytes opaque JSON body]`
//! Total header overhead: exactly 11 bytes.
//!
//! Roles:
//! - Client -> server: [`SignalKind::Join`], [`SignalKind::Bye`].
//! - Peer -> peer (relayed): [`SignalKind::Offer`], [`SignalKind::Answer`],
//!   [`SignalKind::Ice`], [`SignalKind::Signal`].
//! - Server -> client: [`SignalKind::Config`], [`SignalKind::PeerJoined`],
//!   [`SignalKind::PeerLeft`], [`SignalKind::Error`].
//!
//! Bodies stay opaque to the relay. `Config` is the one body the server writes:
//! `{"iceServers":[…],"peers":[…],"self":<authoritative id>}` — the id a client
//! needs before replicated state tells it who it is.
//!
//! Mesh pairing rule: the peer with the **lower** entity id owns the
//! [`RTCPeerConnection`] that creates the offer, so a pair never produces two
//! offers for the same connection.

use std::collections::HashMap;

use bytes::{Buf, BufMut, Bytes, BytesMut};

pub const SIGNAL_MAGIC: u8 = 0x57; // ASCII 'W'
pub const SIGNAL_VERSION: u8 = 1;
pub const SIGNAL_HEADER_LEN: usize = 11;

/// SDP bodies are a couple of KiB; anything past this is a malformed or hostile
/// frame and is rejected at the boundary.
pub const MAX_SIGNAL_PAYLOAD: usize = 64 * 1024;

/// Sender slot for server-originated frames.
pub const SERVER_SENDER: u32 = 0;
/// `to` slot meaning "every voice peer in the room except the sender".
pub const BROADCAST: u32 = u32::MAX;

/// Signaling message kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SignalKind {
    /// Client requests to enter the voice mesh. Body: client capabilities.
    Join = 1,
    /// SDP offer, `to` is a single peer.
    Offer = 2,
    /// SDP answer, `to` is a single peer.
    Answer = 3,
    /// One ICE candidate, `to` is a single peer.
    Ice = 4,
    /// Client leaves the voice mesh; the server tears down peer state.
    Bye = 5,
    /// Server -> client ICE server list (STUN/TURN). Body: JSON array.
    Config = 8,
    /// Server -> clients roster notification. Body: JSON peer info.
    PeerJoined = 9,
    /// Server -> clients roster notification. Body: JSON peer info.
    PeerLeft = 10,
    /// Application-defined negotiation extension, relayed verbatim.
    Signal = 11,
    /// Server -> client rejection of a previous frame. Body: JSON reason.
    Error = 12,
    /// Server -> clients: a speaker became muted (or unmuted). Peer-to-peer media
    /// bypasses the server, so an authoritative mute has to be pushed to the mesh
    /// or it only silences the relay path. Body: `[speaker_id, muted]`.
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

    /// Kinds the server relays between peers rather than consuming itself.
    #[inline]
    pub fn is_relayed(self) -> bool {
        matches!(self, Self::Offer | Self::Answer | Self::Ice | Self::Signal)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalError {
    BufferTooShort { expected: usize, actual: usize },
    InvalidMagic(u8),
    UnsupportedVersion(u8),
    UnknownKind(u8),
    PayloadTooLarge { limit: usize, actual: usize },
}

impl std::fmt::Display for SignalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooShort { expected, actual } => {
                write!(f, "signal frame too short: expected at least {expected}, got {actual}")
            }
            Self::InvalidMagic(m) => {
                write!(f, "invalid signal magic byte: 0x{m:02X} (expected 'W')")
            }
            Self::UnsupportedVersion(v) => write!(f, "unsupported signal protocol version {v}"),
            Self::UnknownKind(k) => write!(f, "unknown signal kind: {k}"),
            Self::PayloadTooLarge { limit, actual } => {
                write!(f, "signal body too large: {actual} bytes exceeds limit of {limit}")
            }
        }
    }
}

impl std::error::Error for SignalError {}

/// A decoded signaling frame. `payload` is the opaque JSON body, shared with the
/// inbound buffer rather than copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalFrame {
    pub version: u8,
    pub kind: SignalKind,
    pub from: u32,
    pub to: u32,
    pub payload: Bytes,
}

impl SignalFrame {
    pub fn new(kind: SignalKind, from: u32, to: u32, payload: Bytes) -> Self {
        Self { version: SIGNAL_VERSION, kind, from, to, payload }
    }

    /// Cheap probe before the full decode — also how the server distinguishes a
    /// signaling frame from an audio frame on a shared message channel.
    #[inline]
    pub fn is_signal(buf: &[u8]) -> bool {
        buf.len() >= SIGNAL_HEADER_LEN && buf[0] == SIGNAL_MAGIC
    }

    /// Kind of an already-magic-checked buffer, without copying anything.
    #[inline]
    pub fn peek_kind(buf: &[u8]) -> Option<SignalKind> {
        if !Self::is_signal(buf) {
            return None;
        }
        SignalKind::from_u8(buf[2])
    }

    /// Zero-copy decode: slices the body out of `buf`, refcount-only.
    pub fn decode(buf: &Bytes) -> Result<Self, SignalError> {
        if buf.len() < SIGNAL_HEADER_LEN {
            return Err(SignalError::BufferTooShort {
                expected: SIGNAL_HEADER_LEN,
                actual: buf.len(),
            });
        }
        if buf[0] != SIGNAL_MAGIC {
            return Err(SignalError::InvalidMagic(buf[0]));
        }
        if buf[1] != SIGNAL_VERSION {
            return Err(SignalError::UnsupportedVersion(buf[1]));
        }
        let kind = SignalKind::from_u8(buf[2]).ok_or(SignalError::UnknownKind(buf[2]))?;
        let body = buf.len() - SIGNAL_HEADER_LEN;
        if body > MAX_SIGNAL_PAYLOAD {
            return Err(SignalError::PayloadTooLarge { limit: MAX_SIGNAL_PAYLOAD, actual: body });
        }
        let mut cursor = &buf[3..SIGNAL_HEADER_LEN];
        Ok(Self {
            version: buf[1],
            kind,
            from: cursor.get_u32(),
            to: cursor.get_u32(),
            payload: buf.slice(SIGNAL_HEADER_LEN..),
        })
    }

    pub fn encode_into(&self, buf: &mut BytesMut) {
        buf.reserve(SIGNAL_HEADER_LEN + self.payload.len());
        buf.put_u8(SIGNAL_MAGIC);
        buf.put_u8(self.version);
        buf.put_u8(self.kind as u8);
        buf.put_u32(self.from);
        buf.put_u32(self.to);
        buf.put_slice(&self.payload);
    }

    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(SIGNAL_HEADER_LEN + self.payload.len());
        self.encode_into(&mut buf);
        buf.freeze()
    }

    /// Total on-wire size.
    #[inline]
    pub fn wire_size(&self) -> usize {
        SIGNAL_HEADER_LEN + self.payload.len()
    }
}

/// Re-emit `frame` as if `authoritative_from` had sent it, sharing the body.
///
/// The client-supplied sender slot is never trusted: it is attacker-chosen and
/// would let one peer speak as another. Copying the 11-byte header and slicing
/// the body keeps this a single small copy plus a refcount bump, independent of
/// SDP size.
pub fn rewrite_sender(frame: &Bytes, authoritative_from: u32) -> Bytes {
    debug_assert!(frame.len() >= SIGNAL_HEADER_LEN);
    let mut buf = BytesMut::with_capacity(frame.len());
    buf.put_slice(&frame[..3]);
    buf.put_u32(authoritative_from);
    buf.put_slice(&frame[7..]);
    buf.freeze()
}

/// Target of a routed frame: the peer id plus the transport session to push it to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalTarget<'a> {
    pub entity_id: u32,
    pub session_id: &'a str,
}

/// Voice mesh membership: entity id <-> transport session, and the routing table
/// for relaying negotiation frames between peers.
///
/// The server holds no SDP state; it only knows which peers exist so it can
/// address a frame and refuse to route to a peer that has left.
#[derive(Debug, Default)]
pub struct VoiceMesh {
    by_entity: HashMap<u32, Box<str>>,
    by_session: HashMap<Box<str>, u32>,
}

impl VoiceMesh {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a voice peer. Returns `true` when the peer was not already in the mesh.
    ///
    /// A reconnect can present either key again (same entity on a new socket, or a
    /// new entity on an old socket), so both indexes are pruned before inserting.
    pub fn join(&mut self, entity_id: u32, session_id: &str) -> bool {
        if let Some(previous) = self.by_entity.get(&entity_id) {
            self.by_session.remove(&**previous);
        }
        if let Some(&previous_entity) = self.by_session.get(session_id) {
            self.by_entity.remove(&previous_entity);
        }
        let is_new = self.by_entity.insert(entity_id, session_id.into()).is_none();
        self.by_session.insert(session_id.into(), entity_id);
        is_new
    }

    /// Remove a peer by entity id, returning the session it was carried on.
    pub fn leave(&mut self, entity_id: u32) -> Option<String> {
        let session = self.by_entity.remove(&entity_id)?;
        self.by_session.remove(&*session);
        Some(session.into())
    }

    /// Remove a peer by transport session (socket close), returning its entity id.
    pub fn leave_session(&mut self, session_id: &str) -> Option<u32> {
        let entity = self.by_session.remove(session_id)?;
        self.by_entity.remove(&entity);
        Some(entity)
    }

    #[inline]
    pub fn session_of(&self, entity_id: u32) -> Option<&str> {
        self.by_entity.get(&entity_id).map(|s| &**s)
    }

    #[inline]
    pub fn entity_of(&self, session_id: &str) -> Option<u32> {
        self.by_session.get(session_id).copied()
    }

    #[inline]
    pub fn contains(&self, entity_id: u32) -> bool {
        self.by_entity.contains_key(&entity_id)
    }

    #[inline]
    pub fn peer_count(&self) -> usize {
        self.by_entity.len()
    }

    /// The lower entity id creates the offer, so a pair never negotiates twice.
    #[inline]
    pub fn should_initiate(from: u32, to: u32) -> bool {
        from < to
    }

    /// Visit every peer, skipping `except`. Allocates nothing; the callback takes
    /// borrowed data so the caller can push straight into the transport.
    pub fn for_each_peer(&self, except: u32, mut f: impl FnMut(SignalTarget<'_>)) {
        for (&entity_id, session) in &self.by_entity {
            if entity_id != except {
                f(SignalTarget { entity_id, session_id: session });
            }
        }
    }

    /// Resolve where a frame from `from` addressed to `to` must go.
    ///
    /// [`BROADCAST`] fans out to every other peer; a specific id is delivered only
    /// if that peer is still in the mesh (a stale target is dropped, not queued).
    /// Returns `false` when nothing was visited, so the caller can skip building
    /// the outbound frame entirely.
    #[inline]
    pub fn route(&self, from: u32, to: u32, mut f: impl FnMut(SignalTarget<'_>)) -> bool {
        if to == BROADCAST {
            let mut found = false;
            self.for_each_peer(from, |target| {
                found = true;
                f(target);
            });
            return found;
        }
        match self.session_of(to) {
            Some(session_id) if to != from => {
                f(SignalTarget { entity_id: to, session_id });
                true
            }
            _ => false,
        }
    }

    /// Peer ids currently in the mesh, ascending. Only used to build the roster a
    /// joiner is handed, so the allocation here is once-per-join.
    pub fn peers_sorted(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.by_entity.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(kind: SignalKind, from: u32, to: u32, body: &str) -> SignalFrame {
        SignalFrame::new(kind, from, to, Bytes::copy_from_slice(body.as_bytes()))
    }

    #[test]
    fn test_signal_codec_roundtrip() {
        let original = frame(SignalKind::Offer, 7, 12, r#"{"sdp":"v=0\r\n"}"#);
        let wire = original.encode();
        assert_eq!(wire.len(), original.wire_size());
        assert_eq!(wire[0], SIGNAL_MAGIC);

        let decoded = SignalFrame::decode(&wire).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(decoded.payload.as_ref(), br#"{"sdp":"v=0\r\n"}"#);
    }

    #[test]
    fn test_signal_rejects_bad_headers() {
        fn corrupt(wire: &Bytes, byte_index: usize, value: u8) -> Bytes {
            let mut copy = BytesMut::from(&wire[..]);
            copy[byte_index] = value;
            copy.freeze()
        }

        let wire = frame(SignalKind::Ice, 1, 2, "x").encode();

        assert_eq!(
            SignalFrame::decode(&corrupt(&wire, 0, b'V')),
            Err(SignalError::InvalidMagic(b'V'))
        );
        assert_eq!(
            SignalFrame::decode(&corrupt(&wire, 1, 9)),
            Err(SignalError::UnsupportedVersion(9))
        );
        assert_eq!(
            SignalFrame::decode(&corrupt(&wire, 2, 77)),
            Err(SignalError::UnknownKind(77))
        );
        assert_eq!(
            SignalFrame::decode(&wire.slice(0..5)),
            Err(SignalError::BufferTooShort { expected: SIGNAL_HEADER_LEN, actual: 5 })
        );
    }

    #[test]
    fn test_signal_rejects_oversized_body() {
        let huge = Bytes::from(vec![b'x'; MAX_SIGNAL_PAYLOAD + 1]);
        let wire = SignalFrame::new(SignalKind::Offer, 1, 2, huge).encode();
        assert_eq!(
            SignalFrame::decode(&wire),
            Err(SignalError::PayloadTooLarge {
                limit: MAX_SIGNAL_PAYLOAD,
                actual: MAX_SIGNAL_PAYLOAD + 1,
            })
        );
    }

    #[test]
    fn test_signal_probe_distinguishes_audio_from_signaling() {
        let signal = frame(SignalKind::Join, 3, BROADCAST, "{}").encode();
        assert!(SignalFrame::is_signal(&signal));
        assert_eq!(SignalFrame::peek_kind(&signal), Some(SignalKind::Join));

        // Opus audio frames start with 'V' and never match.
        let audio = Bytes::from_static(b"V\x00\x00\x00\x01rest-of-an-opus-frame");
        assert!(!SignalFrame::is_signal(&audio));
        assert_eq!(SignalFrame::peek_kind(&audio), None);

        // Truncated garbage is not a signal either.
        assert!(!SignalFrame::is_signal(b"W\x01\x02"));
    }

    #[test]
    fn test_rewrite_sender_shares_payload() {
        let body = Bytes::from_static(b"{\"candidate\":\"spam\"}");
        let forged = SignalFrame::new(SignalKind::Ice, 999, 4, body.clone());
        let wire = forged.encode();
        let rewritten = rewrite_sender(&wire, 1);

        let decoded = SignalFrame::decode(&rewritten).unwrap();
        assert_eq!(decoded.from, 1);
        assert_eq!(decoded.to, 4);
        assert_eq!(decoded.kind, SignalKind::Ice);
        assert_eq!(decoded.payload, body);
    }

    #[test]
    fn test_mesh_routing_targets_live_peers_only() {
        let mut mesh = VoiceMesh::new();
        mesh.join(1, "session-one");
        mesh.join(2, "session-two");
        mesh.join(3, "session-three");

        let mut direct = Vec::new();
        let hit = mesh.route(1, 2, |t| direct.push((t.entity_id, t.session_id.to_string())));
        assert!(hit);
        assert_eq!(direct, vec![(2, "session-two".to_string())]);

        let mut bcast = Vec::new();
        mesh.route(1, BROADCAST, |t| bcast.push(t.entity_id));
        bcast.sort_unstable();
        assert_eq!(bcast, vec![2, 3]);

        // A peer that already left is not a routable target.
        assert_eq!(mesh.leave(3).as_deref(), Some("session-three"));
        assert!(!mesh.route(1, 3, |_| panic!("must not route to a departed peer")));
    }

    #[test]
    fn test_mesh_leave_session_keeps_maps_in_sync() {
        let mut mesh = VoiceMesh::new();
        mesh.join(5, "sock-5");
        assert_eq!(mesh.entity_of("sock-5"), Some(5));

        assert_eq!(mesh.leave_session("sock-5"), Some(5));
        assert_eq!(mesh.entity_of("sock-5"), None);
        assert_eq!(mesh.session_of(5), None);
        assert_eq!(mesh.peer_count(), 0);

        // Re-joining under a new socket evicts the stale reverse entry.
        mesh.join(5, "sock-5-new");
        mesh.join(6, "sock-6");
        assert_eq!(mesh.entity_of("sock-5"), None);
        assert_eq!(mesh.session_of(5), Some("sock-5-new"));
        assert_eq!(mesh.peers_sorted(), vec![5, 6]);
    }

    #[test]
    fn test_initiator_rule_is_symmetric() {
        assert!(VoiceMesh::should_initiate(1, 2));
        assert!(!VoiceMesh::should_initiate(2, 1));
        assert!(!VoiceMesh::should_initiate(4, 4));
    }
}
