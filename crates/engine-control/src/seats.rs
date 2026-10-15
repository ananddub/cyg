//! Seat tokens — the minting side of the wire format in `CONTRACT.md`.
//!
//! ```text
//! token = b64url("<room>:<exp_unix_secs>") + "." + b64url(hmac_sha256(secret, payload))
//! ```
//!
//! `b64url` is RFC 4648 URL-safe *without padding*. The sim plane
//! (`src/threads/seats.rs`) verifies the same bytes, so the two sides are
//! interchangeable and a token minted here authenticates a raw-UDP `JOIN`
//! there. Anything that must not be forgeable is minted here and only here.
//!
//! ## Why the MAC is the authority and the map is a convenience
//!
//! `verify` accepts any token carrying this secret's MAC that has not expired,
//! whether or not this process minted it. That is deliberate: the raw-UDP plane
//! holds no seat map at all, so a seat minted on one control-plane worker has
//! to authenticate on the data plane without a sync step. The map only carries
//! the seats whose `sessionId` must survive a `reconnect`.
//!
//! ## Why the map is bounded
//!
//! Every mint is a page load and page loads get abandoned, so an unbounded map
//! is an unbounded leak driven by the one endpoint that needs no credentials.
//! `MAX_OUTSTANDING_SEATS` caps it: `mint` sweeps expired seats first and then
//! *refuses* rather than growing. Per-IP rate limiting stays the first line of
//! defence; this is the ceiling that makes a bypass of it non-fatal.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Live seats a single registry will hold. Generous next to the transport's own
/// client cap: reaching this means abandoned page loads are outrunning the
/// one-minute sweep.
pub const MAX_OUTSTANDING_SEATS: usize = 50_000;

/// How far `mint` may push a seat's expiry to keep it unique. The frozen
/// payload is `"<room>:<exp_unix_secs>"` and carries no nonce, so two clients
/// minted for the same room in the same second would otherwise be handed the
/// *same* token — and the same `sessionId`, which the game plane reads as one
/// player identity. `mint` therefore bumps the expiry one second at a time until
/// the token is new; the bump only ever *lengthens* the TTL the client is told,
/// and past this many seconds the room has more live seats than a lobby can
/// have, so the mint is refused instead.
pub const MAX_TTL_DRIFT_SECS: u64 = 4096;

/// A seat this plane handed out and is still willing to honour.
///
/// `Eq` because it travels inside the signaling queue types, which are compared
/// in tests: a queued offer must be provably the same seat that minted it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    pub id: String,
    pub room: String,
    pub token: String,
    /// Unix seconds after which the token is refused.
    pub expires_at: u64,
}

pub struct SeatRegistry {
    secret: Vec<u8>,
    by_token: HashMap<String, Seat>,
    /// Highest expiry already handed out per room. Uniqueness comes from the
    /// expiry (see [`MAX_TTL_DRIFT_SECS`]), and a watermark keeps a mint O(1):
    /// probing the map instead would make a coordinated flood against one room
    /// cost quadratically in the seats it mints.
    next_expiry: HashMap<String, u64>,
    minted: u64,
    rejected: u64,
    /// Mints refused by the outstanding-seat bound. Separate from `rejected`
    /// (an auth failure) so a capacity event is not read as an attack.
    refused: u64,
    max_outstanding: usize,
}

impl SeatRegistry {
    pub fn new(secret: &str) -> SeatRegistry {
        SeatRegistry::with_seat_bound(secret, MAX_OUTSTANDING_SEATS)
    }

    /// The same registry with a different ceiling. Tests use it; production
    /// goes through [`SeatRegistry::new`], which applies
    /// [`MAX_OUTSTANDING_SEATS`].
    pub fn with_seat_bound(secret: &str, max_outstanding: usize) -> SeatRegistry {
        SeatRegistry {
            secret: secret.as_bytes().to_vec(),
            by_token: HashMap::new(),
            next_expiry: HashMap::new(),
            minted: 0,
            rejected: 0,
            refused: 0,
            max_outstanding: max_outstanding.max(1),
        }
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// `payload = "<room>:<exp_unix_secs>"`, exact ASCII bytes.
    ///
    /// Returns `None` when the map is at its bound even after sweeping expired
    /// seats, when the expiry needed for a unique token would drift past
    /// [`MAX_TTL_DRIFT_SECS`], and for rooms the payload layout cannot carry:
    /// the verifier splits on the *last* `:` and the raw-UDP plane additionally
    /// refuses a room containing `.`, so minting either would produce a seat
    /// that authenticates nowhere. The route validates names; this rule lives
    /// next to the bytes.
    pub fn mint(&mut self, room: &str, ttl_secs: u64) -> Option<Seat> {
        if room.is_empty() || room.contains(':') || room.contains('.') {
            self.rejected += 1;
            return None;
        }
        if self.by_token.len() >= self.max_outstanding {
            self.retire_expired();
            if self.by_token.len() >= self.max_outstanding {
                self.refused += 1;
                return None;
            }
        }
        // The payload carries no nonce, so two seats for the same room in the
        // same second would be the *same token* — and therefore the same
        // `sessionId`, and therefore one player identity on the game plane.
        // Uniqueness comes from the expiry: always strictly above every expiry
        // this room ever used, which only ever lengthens the TTL the client is
        // told, and only within the drift cap.
        let base = Self::now_secs().saturating_add(ttl_secs);
        let used = self.next_expiry.get(room).map(|e| e.saturating_add(1)).unwrap_or(0);
        let expires_at = base.max(used);
        if expires_at > base.saturating_add(MAX_TTL_DRIFT_SECS) {
            self.refused += 1;
            return None;
        }
        self.next_expiry.insert(room.to_string(), expires_at);
        let seat = Seat {
            id: format!("{}-{}", Self::now_secs(), self.minted),
            room: room.to_string(),
            token: self.sign(room, expires_at),
            expires_at,
        };
        self.minted += 1;
        self.by_token.insert(seat.token.clone(), seat.clone());
        Some(seat)
    }

    /// Recompute the MAC for `room:exp` and assemble the two b64url halves.
    fn sign(&self, room: &str, expires_at: u64) -> String {
        let payload = format!("{room}:{expires_at}");
        let payload = payload.as_bytes();
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.secret)
            .expect("hmac accepts any key length");
        mac.update(payload);
        format!("{}.{}", b64url(payload), b64url(&mac.finalize().into_bytes()))
    }

    /// The seat `token` names, or `None` — counting every refusal.
    ///
    /// The MAC compare is `Mac::verify_slice`, which is constant-time in the
    /// tag and length-checked; nothing about the payload's contents is trusted
    /// (or parsed) before it passes.
    pub fn verify(&mut self, token: &str) -> Option<Seat> {
        match self.check(token) {
            Some(seat) => Some(seat),
            None => {
                self.rejected += 1;
                None
            }
        }
    }

    /// Would this token be accepted right now, without touching the counters.
    pub fn is_valid(&self, token: &str) -> bool {
        self.check(token).is_some()
    }

    fn check(&self, token: &str) -> Option<Seat> {
        let (payload_b64, mac_b64) = token.split_once('.')?;
        // A second dot is a forged layout: the payload never contains one.
        if payload_b64.is_empty() || mac_b64.is_empty() || mac_b64.contains('.') {
            return None;
        }
        let payload = b64url_decode(payload_b64)?;
        let presented = b64url_decode(mac_b64)?;
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.secret)
            .expect("hmac accepts any key length");
        mac.update(&payload);
        if mac.verify_slice(&presented).is_err() {
            return None;
        }
        // Only a verified payload is worth parsing.
        let text = std::str::from_utf8(&payload).ok()?;
        let (room, exp) = text.rsplit_once(':')?;
        let exp: u64 = exp.parse().ok()?;
        if room.is_empty() || room.contains('.') {
            return None;
        }
        if exp <= Self::now_secs() {
            return None;
        }
        Some(self.by_token.get(token).cloned().unwrap_or(Seat {
            id: format!("ext-{room}-{exp}"),
            room: room.to_string(),
            token: token.to_string(),
            expires_at: exp,
        }))
    }

    /// Drop a seat from the map: the client left, or its room was closed.
    /// The token still verifies until it expires — see the module docs on why
    /// the map is a convenience, not the access list.
    pub fn release(&mut self, token: &str) -> Option<Seat> {
        self.by_token.remove(token)
    }

    pub fn retire_expired(&mut self) {
        let now = Self::now_secs();
        self.by_token.retain(|_, seat| seat.expires_at > now);
        // A room whose watermark is in the past has no live token left, so its
        // entry is dead weight: room names churn as fast as pages load.
        self.next_expiry.retain(|_, expiry| *expiry > now);
    }

    /// Seats still inside their TTL, live or not — what `/status` reports.
    pub fn outstanding(&self) -> usize {
        self.by_token.len()
    }

    pub fn counts(&self) -> (u64, u64) {
        (self.minted, self.rejected)
    }

    /// Mints refused because the registry was at its bound.
    pub fn refused(&self) -> u64 {
        self.refused
    }

    /// The bound this registry enforces.
    pub fn seat_bound(&self) -> usize {
        self.max_outstanding
    }
}

/// RFC 4648 URL-safe base64, no padding: `+`→`-`, `/`→`_`, `=` dropped.
pub fn b64url(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[(n >> 6 & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(T[(n & 63) as usize] as char);
        }
    }
    out
}

/// Strict unpadded b64url, matching `src/threads/seats.rs` byte for byte: any
/// character outside the URL-safe alphabet, any `=`, or a `len % 4 == 1`
/// remainder is not a token — it is a probe, and both planes must agree.
pub fn b64url_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 == 1 || bytes.contains(&b'=') {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for group in bytes.chunks(4) {
        let mut acc = 0u32;
        let mut bits = 0;
        for &c in group {
            let v = val(c)?;
            acc = (acc << 6) | v as u32;
            bits += 6;
        }
        acc <<= 24 - bits;
        for i in 0..(bits / 8) {
            out.push((acc >> (16 - i * 8)) as u8 & 0xff);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_config::DEV_SEAT_SECRET;

    #[test]
    fn two_clients_in_the_same_second_get_different_seats() {
        // The payload has no nonce, so without the expiry bump both clients get
        // the same token, the same sessionId, and one player identity on the
        // game plane. That is the bug this pins down.
        let mut r = SeatRegistry::new("secret");
        let a = r.mint("tank-3d", 300).unwrap();
        let b = r.mint("tank-3d", 300).unwrap();
        assert_ne!(a.token, b.token, "the token is the identity; it cannot repeat");
        assert_ne!(a.id, b.id);
        assert_eq!(r.outstanding(), 2);
        assert!(b.expires_at >= a.expires_at, "the bump only lengthens the TTL");
        // Each resumes its own seat. (The cross-plane check against the raw-UDP
        // verifier lives in the sim crate and is covered there — integration TODO.)
        assert_eq!(r.verify(&a.token).unwrap().id, a.id);
        assert_eq!(r.verify(&b.token).unwrap().id, b.id);
    }

    #[test]
    fn the_drift_cap_refuses_instead_of_handing_out_forever_seats() {
        let mut r = SeatRegistry::new("secret");
        for i in 0..=MAX_TTL_DRIFT_SECS {
            assert!(r.mint("tank-3d", 1).is_some(), "mint {i}");
        }
        assert_eq!(r.refused(), 0);
        assert!(r.mint("tank-3d", 1).is_none(), "past the cap the mint stops");
        assert_eq!(r.refused(), 1);
        assert_eq!(r.counts().0, MAX_TTL_DRIFT_SECS + 1);
        // Another room is unaffected: the payload, and so the token, differs.
        assert!(r.mint("arena_final", 1).is_some());
    }

    #[test]
    fn mints_and_verifies_its_own_token() {
        let mut r = SeatRegistry::new("secret");
        let seat = r.mint("tank-3d", 300).unwrap();
        assert_eq!(r.verify(&seat.token).unwrap().room, "tank-3d");
    }

    #[test]
    fn rejects_a_tampered_payload() {
        let mut r = SeatRegistry::new("secret");
        let seat = r.mint("tank-3d", 300).unwrap();
        let (_, mac) = seat.token.split_once('.').unwrap();
        // Same MAC, different room: the payload is not covered by it any more.
        let forged = format!("{}.{mac}", b64url(b"other-room:9999999999"));
        assert!(r.verify(&forged).is_none());
    }

    #[test]
    fn rejects_an_expired_seat_without_trusting_the_payload() {
        let mut r = SeatRegistry::new("secret");
        let token = r.sign("tank-3d", 1);
        assert!(r.verify(&token).is_none());
    }

    #[test]
    fn rejects_a_token_signed_with_another_secret() {
        let other = SeatRegistry::new(DEV_SEAT_SECRET).sign("tank-3d", 4_000_000_000);
        let mut r = SeatRegistry::new("a-different-secret");
        assert!(r.verify(&other).is_none());
    }

    #[test]
    fn every_refusal_is_counted() {
        let mut r = SeatRegistry::new("secret");
        let good = r.mint("tank-3d", 300).unwrap();
        let bad = [
            "",
            "no-dot",
            "a.b",
            "@@@.###",
            "AA==.AA==",
            &format!("{}.", good.token),
        ];
        for token in bad {
            assert!(r.verify(token).is_none(), "must reject {token:?}");
        }
        let (minted, rejected) = r.counts();
        assert_eq!(minted, 1);
        assert_eq!(rejected, bad.len() as u64, "one bump per refused token");
    }

    #[test]
    fn a_released_seat_is_gone_from_the_map_but_still_authenticates() {
        let mut r = SeatRegistry::new("secret");
        let seat = r.mint("tank-3d", 300).unwrap();
        assert_eq!(r.outstanding(), 1);
        assert_eq!(r.release(&seat.token).unwrap().id, seat.id);
        assert_eq!(r.outstanding(), 0);
        // The MAC, not the map, is the authority (see the module docs).
        assert_eq!(r.verify(&seat.token).unwrap().room, "tank-3d");
    }

    #[test]
    fn retire_expired_keeps_live_seats() {
        let mut r = SeatRegistry::new("secret");
        let live = r.mint("tank-3d", 600).unwrap();
        let dead = r.sign("tank-3d", 1);
        r.by_token.insert(
            dead.clone(),
            Seat {
                id: "dead".into(),
                room: "tank-3d".into(),
                token: dead.clone(),
                expires_at: 1,
            },
        );
        assert_eq!(r.outstanding(), 2);
        r.retire_expired();
        assert_eq!(r.outstanding(), 1);
        assert!(r.by_token.contains_key(&live.token));
        assert!(!r.by_token.contains_key(&dead));
    }

    #[test]
    fn session_id_is_stable_across_repeated_verifies() {
        // `reconnect` must hand back the sessionId the SDK already put into its
        // WebSocket upgrade query, twice over if the page reloads twice.
        let mut r = SeatRegistry::new("secret");
        let seat = r.mint("tank-3d", 300).unwrap();
        let first = r.verify(&seat.token).unwrap();
        let second = r.verify(&seat.token).unwrap();
        assert_eq!(first, seat);
        assert_eq!(first.id, second.id);
        assert_eq!(first.token, second.token);
    }

    #[test]
    fn minting_is_bounded_and_says_so() {
        let mut r = SeatRegistry::with_seat_bound("secret", 4);
        for _ in 0..4 {
            assert!(r.mint("tank-3d", 300).is_some());
        }
        assert_eq!(r.outstanding(), 4);
        assert_eq!(r.seat_bound(), 4);
        assert!(r.mint("tank-3d", 300).is_none(), "the bound is enforced");
        assert_eq!(r.refused(), 1);
        assert_eq!(r.outstanding(), 4, "a refused mint stores nothing");
        assert_eq!(r.counts().0, 4, "and is not counted as a mint");
        // Freeing space lets minting resume: the bound is a ceiling, not a latch.
        let some = r.by_token.keys().next().cloned().unwrap();
        r.release(&some);
        assert!(r.mint("tank-3d", 300).is_some());
    }

    #[test]
    fn the_bound_sweeps_expired_seats_instead_of_refusing() {
        let mut r = SeatRegistry::with_seat_bound("secret", 2);
        let old = r.mint("tank-3d", 300).unwrap();
        r.by_token.get_mut(&old.token).unwrap().expires_at = 1; // already dead
        assert!(r.mint("tank-3d", 300).is_some());
        assert_eq!(r.outstanding(), 2);
        // Full, but one entry is expired: mint sweeps, then succeeds.
        assert!(r.mint("tank-3d", 300).is_some());
        assert_eq!(r.refused(), 0);
        assert_eq!(r.outstanding(), 2);
    }

    #[test]
    fn mint_refuses_rooms_the_payload_layout_cannot_carry() {
        let mut r = SeatRegistry::new("secret");
        assert!(r.mint("", 300).is_none());
        assert!(r.mint("a:b", 300).is_none());
        assert!(r.mint("a.b", 300).is_none());
        assert_eq!(r.outstanding(), 0);
    }

    #[test]
    fn base64_round_trips_every_length() {
        for len in 1..12 {
            let data: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
            assert_eq!(b64url_decode(&b64url(&data)).unwrap(), data, "len {len}");
        }
    }

    #[test]
    fn base64_matches_rfc4648_urlsafe_vectors() {
        assert_eq!(b64url(b""), "");
        assert_eq!(b64url(b"f"), "Zg");
        assert_eq!(b64url(b"fo"), "Zm8");
        assert_eq!(b64url(b"foo"), "Zm9v");
        assert_eq!(b64url(b"foob"), "Zm9vYg");
        assert_eq!(b64url(b"fooba"), "Zm9vYmE");
        assert_eq!(b64url(b"foobar"), "Zm9vYmFy");
        // The two characters that differ from standard base64.
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn decoder_rejects_padded_empty_and_trailing_input() {
        assert_eq!(b64url_decode("Zm9v"), Some(b"foo".to_vec()));
        assert!(b64url_decode("").is_none());
        assert!(b64url_decode("Zm9v=").is_none());
        assert!(b64url_decode("Q").is_none());
        assert!(b64url_decode("Zm9+").is_none());
    }

    // ---- wire format fixed vector -------------------------------------------
    //
    // The raw-UDP plane (`src/threads/seats.rs`, engine-server) is the verifier
    // that authenticates `JOIN [0x20][token_len u8][token]` on :7778. Its
    // bidirectional cross-plane tests cannot live here (engine-control must not
    // depend on engine-server); they are tracked as an integration TODO. The
    // byte layout itself is still pinned on this side by the fixed vector below.

    #[test]
    fn a_fixed_vector_pins_the_token_bytes() {
        // Computed independently (OpenSSL/Node semantics, no Rust involved):
        //   payload = "tank-3d:1700000000"
        //   mac     = HMAC-SHA256("ox-dev-seat-secret", payload bytes)
        //   token   = b64url(payload) + "." + b64url(mac), unpadded
        let r = SeatRegistry::new(DEV_SEAT_SECRET);
        assert_eq!(
            r.sign("tank-3d", 1_700_000_000),
            "dGFuay0zZDoxNzAwMDAwMDAw.4y2xJ1Nc7shWlds8ZcQPA_ntwg3irdni9ltMKvRIOkE"
        );
        assert_eq!(
            r.sign("arena_final", 1_800_000_000),
            "YXJlbmFfZmluYWw6MTgwMDAwMDAwMA.o5CjkR7hxLmyPiL_M6HR6p-4HY3oXjn-x_lHMyb1zfE"
        );
    }
}
