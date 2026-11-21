// Verified mirror of the bounds skeleton of
// `crates/engine-server/src/rooms/game_room/match_outcome.rs::decode`.
//
// Keep in sync with the original. What is mirrored and verified: the exact
// sequential byte-walk (version byte, length-prefixed leaderboard, participant
// count gates, per-participant 16+8+8 + length-prefixed metadata + stat table)
// with the same rejection rules — so "decode never reads out of bounds" is
// proven, along with `1 <= participants <= 64` on success and
// `cursor == bytes.len()` (no trailing bytes tolerated).
//
// NOT mirrored: UTF-8 validation of names/metadata and the String allocation
// itself (the mirror validates structure only). Behavior parity on valid
// inputs is pinned by tests/verus_parity.rs against the real `decode`.

#[cfg_attr(verus, verus_verify)]
pub const MO_MAX_PARTICIPANTS: usize = 64;
#[cfg_attr(verus, verus_verify)]
pub const MO_MAX_STAT_NAME: usize = 64;
#[cfg_attr(verus, verus_verify)]
pub const MO_MAX_METADATA: usize = 4096;
#[cfg_attr(verus, verus_verify)]
pub const MO_MAX_RESULT_BYTES: usize = 1024 * 1024;

/// Why a reported result was not usable (structural subset of `BadResult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus, verus_verify)]
pub enum MoErr {
    Truncated,
    BadVersion,
    NoParticipants,
    TooManyParticipants,
    StatNameTooLong,
    MetadataTooLarge,
    TrailingBytes,
}

/// Verified `Reader::take`: advance `at` by `n`, `None` on overflow or when
/// the end would pass `bytes.len()`.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        at <= bytes.len(),
        bytes.len() <= 1048576usize,
    ensures
        match r {
            Some(next) => at <= next && next <= bytes.len(),
            None => true,
        }
))]
pub fn mo_take(bytes: &[u8], at: usize, n: usize) -> Option<usize> {
    let end = at.checked_add(n)?;
    if end > bytes.len() {
        return Option::None;
    }
    Option::Some(end)
}

/// Verified length-prefixed string skip (same cursor math as `Reader::string`,
/// without the UTF-8 check).
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        at <= bytes.len(),
        bytes.len() <= 1048576usize,
    ensures
        match r {
            Some(next) => at <= next && next <= bytes.len(),
            None => true,
        }
))]
pub fn mo_skip_string(bytes: &[u8], at: usize) -> Option<usize> {
    let b0 = *bytes.get(at)?;
    let b1 = *bytes.get(at + 1)?;
    let len = ((b0 as u16) + ((b1 as u16) * 256)) as usize;
    mo_take(bytes, at + 2, len)
}

/// Walk one participant record (16-byte id, two i64s, length-prefixed
/// metadata, length-checked stat table). Mirrors the per-participant half of
/// `decode`, including the metadata and stat-name gates.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        at0 <= bytes.len(),
        bytes.len() <= 1048576usize,
    ensures
        match r {
            Some(next) => at0 <= next && next <= bytes.len(),
            None => true,
        }
))]
pub fn mo_participant(bytes: &[u8], at0: usize) -> Option<usize> {
    // user_id (16) + score (8) + subscore (8)
    let mut at = mo_take(bytes, at0, 32)?;
    // metadata
    let b0 = *bytes.get(at)?;
    let b1 = *bytes.get(at + 1)?;
    let meta_len = ((b0 as u16) + ((b1 as u16) * 256)) as usize;
    at = mo_take(bytes, at + 2, meta_len)?;
    if meta_len > 0 && meta_len > MO_MAX_METADATA {
        return Option::None;
    }
    // stat table
    let b0 = *bytes.get(at)?;
    let b1 = *bytes.get(at + 1)?;
    let stat_count = ((b0 as u16) + ((b1 as u16) * 256)) as usize;
    at = at + 2;
    let mut i = 0usize;
    #[cfg_attr(verus, verus_spec(
        invariant
            i <= stat_count,
            at0 <= at,
            at <= bytes.len(),
            bytes.len() <= 1048576usize,
        decreases
            stat_count - i,
    ))]
    while i < stat_count {
        // stat name: length-checked, non-empty, then skipped
        let b0 = *bytes.get(at)?;
        let b1 = *bytes.get(at + 1)?;
        let name_len = ((b0 as u16) + ((b1 as u16) * 256)) as usize;
        if name_len == 0 || name_len > MO_MAX_STAT_NAME {
            return Option::None;
        }
        at = mo_take(bytes, at + 2, name_len)?;
        // delta i64
        at = mo_take(bytes, at, 8)?;
        i += 1;
    }
    Option::Some(at)
}

/// Verified skeleton of `decode`: walk the whole result, reject with the same
/// rules, succeed only when every byte is consumed. Returns the participant
/// count on success.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        bytes.len() <= MO_MAX_RESULT_BYTES as int,
    ensures
        match r {
            Ok(count) => 1 <= count && count <= MO_MAX_PARTICIPANTS as int,
            Err(_) => true,
        }
))]
pub fn mo_validate(bytes: &[u8]) -> Result<usize, MoErr> {
    let n = bytes.len();
    // version byte
    if bytes.len() < 1 {
        return Result::Err(MoErr::Truncated);
    }
    if bytes[0] != 1 {
        return Result::Err(MoErr::BadVersion);
    }
    // leaderboard
    let mut at = match mo_skip_string(bytes, 1) {
        Option::Some(next) => next,
        Option::None => return Result::Err(MoErr::Truncated),
    };
    // participant count, gated before any body is read
    if at + 2 > bytes.len() {
        return Result::Err(MoErr::Truncated);
    }
    let b0 = bytes[at] as u16;
    let b1 = bytes[at + 1] as u16;
    let count = (b0 + (b1 * 256)) as usize;
    at = at + 2;
    if count == 0 {
        return Result::Err(MoErr::NoParticipants);
    }
    if count > MO_MAX_PARTICIPANTS {
        return Result::Err(MoErr::TooManyParticipants);
    }
    // participant records
    let mut i = 0usize;
    #[cfg_attr(verus, verus_spec(
        invariant
            i <= count,
            at <= bytes.len(),
            bytes.len() <= 1048576usize,
        decreases
            count - i,
    ))]
    while i < count {
        at = match mo_participant(bytes, at) {
            Option::Some(next) => next,
            Option::None => return Result::Err(MoErr::Truncated),
        };
        i += 1;
    }
    // no trailing bytes
    if at != bytes.len() {
        return Result::Err(MoErr::TrailingBytes);
    }
    Result::Ok(count)
}
