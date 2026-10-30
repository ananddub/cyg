use chrono::{DateTime, Utc};
use uuid::Uuid;

/// The arm an account is told about when a flag defines none of its own and the
/// account is inside the rollout.
pub const TREATMENT: &str = "treatment";
/// The arm an account is told about when it is outside the rollout, and the
/// first arm of a flag that names its own.
pub const CONTROL: &str = "control";

/// Which arm one account lands in, and whether that arm is the one that does
/// something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub variant: String,
    pub on: bool,
}

/// `hash(flag_key + ":" + salt + ":" + user_id) % 100`, where `hash` is FNV-1a
/// over the UTF-8 bytes. The account id is the whole input, so an account in a
/// rollout stays in it for as long as the flag keeps this salt, and changing the
/// salt is the only way to move accounts between the arms of one flag.
pub fn bucket(flag_key: &str, salt: &str, user_id: Uuid) -> u32 {
    (fnv1a(&format!("{flag_key}:{salt}:{user_id}")) % 100) as u32
}

pub fn eligible(flag_key: &str, salt: &str, user_id: Uuid, rollout_pct: u32) -> bool {
    rollout_pct >= 100 || bucket(flag_key, salt, user_id) < rollout_pct
}

/// Whether a flag's scheduled window is open at `now`. A window that has closed
/// is not a deleted flag: the experiment keeps its salt and its arms, so the
/// accounts it was measuring come back if it is ever opened again.
pub fn within_window(
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    from.is_none_or(|from| from <= now) && to.is_none_or(|to| now < to)
}

/// The whole decision, so that gating code and `GET /v2/flags` cannot disagree
/// about what one account was told. `arms` are the flag's variants ordered by
/// name and the first of them is the control; `pinned` is what an override says.
/// `enabled` is the flag switched on *and* its window open: the two states that
/// answer the control for everybody.
pub fn place(
    flag_key: &str,
    salt: &str,
    user_id: Uuid,
    enabled: bool,
    rollout_pct: u32,
    arms: &[(String, u32)],
    pinned: Option<&str>,
) -> Assignment {
    let control = arms
        .first()
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| CONTROL.to_owned());
    if !enabled {
        return Assignment {
            variant: control,
            on: false,
        };
    }
    // A pin that names an arm nobody has is a mistake the operator made: serving
    // it would tell a player about a test that does not exist.
    if let Some(pinned) = pinned
        && (arms.is_empty() || arms.iter().any(|(name, _)| name == pinned))
    {
        return Assignment {
            variant: pinned.to_owned(),
            on: pinned != control,
        };
    }
    if !eligible(flag_key, salt, user_id, rollout_pct) {
        return Assignment {
            variant: control,
            on: false,
        };
    }
    let total: u64 = arms.iter().map(|(_, weight)| u64::from(*weight)).sum();
    if arms.is_empty() {
        return Assignment {
            variant: TREATMENT.to_owned(),
            on: true,
        };
    }
    if total == 0 {
        return Assignment {
            variant: control,
            on: false,
        };
    }
    // A second label on the same hash: which arm an account lands in must not be
    // correlated with whether it landed in the rollout at all.
    let mut target = fnv1a(&format!("{flag_key}:{salt}:{user_id}:arm")) % total;
    for (name, weight) in arms {
        if target < u64::from(*weight) {
            return Assignment {
                variant: name.clone(),
                on: *name != control,
            };
        }
        target -= u64::from(*weight);
    }
    Assignment {
        variant: control,
        on: false,
    }
}

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a, chosen because it is a dozen lines with no dependency and because an
/// assignment has to survive a restart, which a randomly seeded hasher does not.
fn fnv1a(input: &str) -> u64 {
    let mut hash = OFFSET;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
