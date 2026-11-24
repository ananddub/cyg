//! What a room reported when its match ended.
//!
//! A guest knows who won; the control plane knows how to store it. The guest
//! builds the result (`Room.endMatch(..)`), the host reads those bytes and puts
//! them on the bus as part of the match report, and this module turns them back
//! into the shape `record_result` already takes — so a result that came from a
//! room passes exactly the same checks as one that came over REST, and a guest
//! never holds a server key or a network handle.
//!
//! Layout (`on_match_end` returns `(ptr << 32) | len`):
//!
//! ```text
//! [u8 1]                       version
//! [u16 leaderboardLen][utf8]   empty when the round scores to no board
//! [u16 count]                  participants, 1..=64
//!   { [16 userId] [i64 score] [i64 subscore]
//!     [u16 metadataLen][utf8 json]
//!     [u16 statCount] { [u16 nameLen][utf8 name] [i64 delta] }* }
//! ```
//!
//! Little-endian throughout, matching the rest of the room wire.

use uuid::Uuid;

/// The most players one result may name, matching the service's own limit.
pub const MAX_PARTICIPANTS: usize = 64;
/// How long a stat name may be. Longer is refused, not cut: a truncated name
/// would silently raise a counter nobody asked for.
pub const MAX_STAT_NAME: usize = 64;
const MAX_METADATA: usize = 4096;
/// The most bytes a guest may hand back at all. A result naming every player
/// with every stat stays far below this; the cap exists so a wild pointer or a
/// runaway loop in the guest answers "no" instead of allocating whatever it
/// claimed.
pub const MAX_RESULT_BYTES: usize = 1024 * 1024;

/// One player's outcome.
#[derive(Debug, Clone)]
pub struct Participant {
    pub user_id: Uuid,
    pub score: i64,
    pub subscore: i64,
    pub metadata: Option<String>,
    pub stats: Vec<(String, i64)>,
}

/// A round's result, as the room described it.
#[derive(Debug, Clone)]
pub struct MatchOutcome {
    pub leaderboard: Option<String>,
    pub participants: Vec<Participant>,
}

/// Why a reported result was not usable.
#[derive(Debug)]
pub enum BadResult {
    TooShort,
    UnknownVersion(u8),
    Truncated,
    NoParticipants,
    TooManyParticipants(usize),
    StatNameTooLong(usize),
    MetadataTooLarge(usize),
    TrailingBytes(usize),
}

impl std::fmt::Display for BadResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BadResult::TooShort => write!(f, "the result was shorter than its header"),
            BadResult::UnknownVersion(v) => write!(f, "unknown result version {v}"),
            BadResult::Truncated => write!(f, "the result ended mid-field"),
            BadResult::NoParticipants => write!(f, "the result named no players"),
            BadResult::TooManyParticipants(n) => {
                write!(f, "the result named {n} players (max {MAX_PARTICIPANTS})")
            }
            BadResult::StatNameTooLong(n) => {
                write!(f, "a stat name was {n} bytes (max {MAX_STAT_NAME})")
            }
            BadResult::MetadataTooLarge(n) => {
                write!(f, "metadata was {n} bytes (max {MAX_METADATA})")
            }
            BadResult::TrailingBytes(n) => write!(f, "{n} bytes were left over"),
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], BadResult> {
        let end = self.at.checked_add(n).ok_or(BadResult::Truncated)?;
        if end > self.bytes.len() {
            return Err(BadResult::Truncated);
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, BadResult> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, BadResult> {
        let raw = self.take(2)?;
        Ok(u16::from_le_bytes([raw[0], raw[1]]))
    }

    fn i64(&mut self) -> Result<i64, BadResult> {
        let raw = self.take(8)?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(raw);
        Ok(i64::from_le_bytes(buf))
    }

    fn uuid(&mut self) -> Result<Uuid, BadResult> {
        let raw = self.take(16)?;
        let mut buf = [0u8; 16];
        buf.copy_from_slice(raw);
        Ok(Uuid::from_bytes(buf))
    }

    /// A length-prefixed string. Invalid UTF-8 is refused rather than replaced:
    /// a stat name that came through mangled would raise a counter nobody asked
    /// for, and the room would never learn it did.
    fn string(&mut self) -> Result<String, BadResult> {
        let len = self.u16()? as usize;
        let raw = self.take(len)?;
        String::from_utf8(raw.to_vec()).map_err(|_| BadResult::Truncated)
    }
}

/// Read what a room reported.
pub fn decode(bytes: &[u8]) -> Result<MatchOutcome, BadResult> {
    let mut reader = Reader::new(bytes);
    let version = reader.u8().map_err(|_| BadResult::TooShort)?;
    if version != 1 {
        return Err(BadResult::UnknownVersion(version));
    }

    let leaderboard = reader.string()?;
    let leaderboard = if leaderboard.trim().is_empty() {
        None
    } else {
        Some(leaderboard)
    };

    let count = reader.u16()? as usize;
    if count == 0 {
        return Err(BadResult::NoParticipants);
    }
    if count > MAX_PARTICIPANTS {
        return Err(BadResult::TooManyParticipants(count));
    }

    let mut participants = Vec::with_capacity(count);
    for _ in 0..count {
        let user_id = reader.uuid()?;
        let score = reader.i64()?;
        let subscore = reader.i64()?;
        let metadata = reader.string()?;
        let metadata = if metadata.trim().is_empty() {
            None
        } else {
            if metadata.len() > MAX_METADATA {
                return Err(BadResult::MetadataTooLarge(metadata.len()));
            }
            Some(metadata)
        };

        let stat_count = reader.u16()? as usize;
        let mut stats = Vec::with_capacity(stat_count);
        for _ in 0..stat_count {
            let name = reader.string()?;
            if name.is_empty() || name.len() > MAX_STAT_NAME {
                return Err(BadResult::StatNameTooLong(name.len()));
            }
            let delta = reader.i64()?;
            stats.push((name, delta));
        }

        participants.push(Participant {
            user_id,
            score,
            subscore,
            metadata,
            stats,
        });
    }

    if reader.at != bytes.len() {
        return Err(BadResult::TrailingBytes(bytes.len() - reader.at));
    }

    Ok(MatchOutcome {
        leaderboard,
        participants,
    })
}

impl MatchOutcome {
    /// The request shape `record_result` already validates.
    ///
    /// Metadata arrives as JSON text and is parsed here rather than carried as
    /// a string: the REST endpoint receives a JSON object, and a result that
    /// went through the room must not be stored as `"{\"hero\":\"mage\"}"`.
    /// A guest that wrote invalid JSON loses the metadata, not the score —
    /// the round happened either way.
    pub fn into_request(self) -> crate::dto::match_result::MatchResultRequest {
        use crate::dto::match_result::{MatchResultRequest, ParticipantResult};
        MatchResultRequest {
            leaderboard: self.leaderboard,
            participants: self
                .participants
                .into_iter()
                .map(|part| ParticipantResult {
                    user_id: part.user_id,
                    score: part.score,
                    subscore: part.subscore,
                    stats: part.stats.into_iter().collect(),
                    metadata: part
                        .metadata
                        .and_then(|raw| serde_json::from_str(&raw).ok()),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(leaderboard: &str, parts: &[(Uuid, i64, i64, &str, &[(&str, i64)])]) -> Vec<u8> {
        let mut out = vec![1u8];
        let lb = leaderboard.as_bytes();
        out.extend_from_slice(&(lb.len() as u16).to_le_bytes());
        out.extend_from_slice(lb);
        out.extend_from_slice(&(parts.len() as u16).to_le_bytes());
        for (id, score, subscore, meta, stats) in parts {
            out.extend_from_slice(id.as_bytes());
            out.extend_from_slice(&score.to_le_bytes());
            out.extend_from_slice(&subscore.to_le_bytes());
            let m = meta.as_bytes();
            out.extend_from_slice(&(m.len() as u16).to_le_bytes());
            out.extend_from_slice(m);
            out.extend_from_slice(&(stats.len() as u16).to_le_bytes());
            for (name, delta) in *stats {
                let n = name.as_bytes();
                out.extend_from_slice(&(n.len() as u16).to_le_bytes());
                out.extend_from_slice(n);
                out.extend_from_slice(&delta.to_le_bytes());
            }
        }
        out
    }

    fn id(last: u8) -> Uuid {
        let mut raw = [0u8; 16];
        raw[15] = last;
        Uuid::from_bytes(raw)
    }

    #[test]
    fn a_full_result_round_trips() {
        let bytes = encode(
            "weekly_kills",
            &[
                (
                    id(1),
                    1200,
                    0,
                    r#"{"hero":"mage"}"#,
                    &[("kills", 7), ("wins", 1)],
                ),
                (id(2), 300, 5, "", &[("kills", 2)]),
            ],
        );
        let outcome = decode(&bytes).expect("a well-formed result decodes");
        assert_eq!(outcome.leaderboard.as_deref(), Some("weekly_kills"));
        assert_eq!(outcome.participants.len(), 2);

        let first = &outcome.participants[0];
        assert_eq!(first.user_id, id(1));
        assert_eq!(first.score, 1200);
        assert_eq!(first.metadata.as_deref(), Some(r#"{"hero":"mage"}"#));
        assert_eq!(first.stats, vec![("kills".into(), 7), ("wins".into(), 1)]);

        // Empty metadata is absent, not an empty string: the service writes
        // `metadata` only when there is something to write.
        assert_eq!(outcome.participants[1].metadata, None);
    }

    #[test]
    fn no_board_is_a_result_without_a_board() {
        let bytes = encode("", &[(id(1), 10, 0, "", &[])]);
        let outcome = decode(&bytes).expect("decodes");
        assert_eq!(outcome.leaderboard, None);
        assert_eq!(outcome.participants.len(), 1);
    }

    #[test]
    fn negative_scores_survive() {
        // A golf-style board reports a lower score, and a penalty is a real
        // value — not a sign flip waiting to happen.
        let bytes = encode("", &[(id(1), -42, -7, "", &[])]);
        let outcome = decode(&bytes).expect("decodes");
        assert_eq!(outcome.participants[0].score, -42);
        assert_eq!(outcome.participants[0].subscore, -7);
    }

    #[test]
    fn a_truncated_result_is_refused_not_guessed() {
        let bytes = encode("weekly", &[(id(1), 10, 0, "", &[])]);
        for cut in 1..bytes.len() {
            assert!(
                decode(&bytes[..cut]).is_err(),
                "a result cut at {cut} bytes must not decode"
            );
        }
    }

    #[test]
    fn a_result_with_no_players_is_refused() {
        let bytes = encode("", &[]);
        assert!(matches!(decode(&bytes), Err(BadResult::NoParticipants)));
    }

    #[test]
    fn too_many_players_is_refused_by_count_not_by_reading_them() {
        // Build a header claiming 1000 players with no bodies behind it. The
        // count is checked first, so this is a clear refusal rather than a
        // truncation error found a thousand allocations later.
        let mut bytes = vec![1u8, 0, 0];
        bytes.extend_from_slice(&1000u16.to_le_bytes());
        assert!(matches!(
            decode(&bytes),
            Err(BadResult::TooManyParticipants(1000))
        ));
    }

    #[test]
    fn an_unknown_version_is_refused() {
        let mut bytes = encode("", &[(id(1), 1, 0, "", &[])]);
        bytes[0] = 2;
        assert!(matches!(decode(&bytes), Err(BadResult::UnknownVersion(2))));
    }

    #[test]
    fn trailing_bytes_are_refused() {
        let mut bytes = encode("", &[(id(1), 1, 0, "", &[])]);
        bytes.push(0);
        assert!(matches!(decode(&bytes), Err(BadResult::TrailingBytes(1))));
    }

    #[test]
    fn an_empty_stat_name_is_refused() {
        let bytes = encode("", &[(id(1), 1, 0, "", &[("", 1)])]);
        assert!(decode(&bytes).is_err());
    }
}
