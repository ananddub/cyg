//! Keyed secrets and the dual-key window that lets one be rotated without
//! logging every session out.
//!
//! Env is read once per process, like every other `OX_*` setting, so the
//! previous key exists only in the process that booted with it configured.
//! That is the whole rotation model: deploy with both keys, let the old tokens
//! expire, deploy again with only the new one.

use std::sync::OnceLock;

/// Label on the key that mints today. Tokens minted before key ids existed carry
/// no `kid` and are verified with the current key, so a first rotation does not
/// invalidate sessions that outlive the upgrade.
pub const CURRENT_KEY_ID: &str = "current";
/// Label on the key that is being retired.
pub const PREVIOUS_KEY_ID: &str = "previous";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn or_default(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_owned()
    } else {
        value
    }
}

/// Byte-for-byte comparison that does not short-circuit. A server key and a seat
/// HMAC are both secrets: returning early on the first differing byte leaks how
/// much of a guess was right.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

#[derive(Debug, Clone)]
pub struct Rotation {
    pub signing_key_id: String,
    pub previous_signing_key_id: String,
    pub previous_signing_key: String,
    pub previous_refresh_signing_key: String,
    pub previous_server_key: String,
    pub previous_seat_secret: String,
}

impl Default for Rotation {
    fn default() -> Rotation {
        Rotation {
            signing_key_id: CURRENT_KEY_ID.to_owned(),
            previous_signing_key_id: PREVIOUS_KEY_ID.to_owned(),
            previous_signing_key: String::new(),
            previous_refresh_signing_key: String::new(),
            previous_server_key: String::new(),
            previous_seat_secret: String::new(),
        }
    }
}

impl Rotation {
    pub fn global() -> &'static Rotation {
        static GLOBAL: OnceLock<Rotation> = OnceLock::new();
        GLOBAL.get_or_init(|| Rotation {
            signing_key_id: or_default(env("OX_SIGNING_KEY_ID"), CURRENT_KEY_ID),
            previous_signing_key_id: or_default(env("OX_PREVIOUS_SIGNING_KEY_ID"), PREVIOUS_KEY_ID),
            previous_signing_key: env("OX_PREVIOUS_SIGNING_KEY"),
            previous_refresh_signing_key: env("OX_PREVIOUS_REFRESH_SIGNING_KEY"),
            previous_server_key: env("OX_PREVIOUS_SERVER_KEY"),
            previous_seat_secret: env("OX_PREVIOUS_SEAT_SECRET"),
        })
    }

    /// True while any retired secret is still being accepted.
    pub fn rotating(&self) -> bool {
        !self.previous_signing_key.is_empty()
            || !self.previous_refresh_signing_key.is_empty()
            || !self.previous_server_key.is_empty()
            || !self.previous_seat_secret.is_empty()
    }

    /// Every secret still trusted, for `doctor` to size and report.
    pub fn previous_secrets(&self) -> Vec<(&'static str, &str)> {
        [
            ("OX_PREVIOUS_SIGNING_KEY", &self.previous_signing_key),
            (
                "OX_PREVIOUS_REFRESH_SIGNING_KEY",
                &self.previous_refresh_signing_key,
            ),
            ("OX_PREVIOUS_SERVER_KEY", &self.previous_server_key),
            ("OX_PREVIOUS_SEAT_SECRET", &self.previous_seat_secret),
        ]
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(name, value)| (name, value.as_str()))
        .collect()
    }

    pub fn accepts_server_key(&self, current: &str, presented: &str) -> bool {
        constant_time_eq(presented, current)
            || (!self.previous_server_key.is_empty()
                && constant_time_eq(presented, &self.previous_server_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_server_keys_are_accepted_while_the_window_is_open() {
        let rotation = Rotation {
            previous_server_key: "old-key".into(),
            ..Default::default()
        };
        assert!(rotation.accepts_server_key("new-key", "new-key"));
        assert!(rotation.accepts_server_key("new-key", "old-key"));
        assert!(!rotation.accepts_server_key("new-key", "other"));
        assert!(
            !Rotation::default().accepts_server_key("new-key", "old-key"),
            "after the window closes the retired key must stop working"
        );
    }

    #[test]
    fn constant_time_eq_rejects_length_and_content() {
        assert!(constant_time_eq("", ""));
        assert!(constant_time_eq("secret", "secret"));
        assert!(!constant_time_eq("secret", "secre"));
        assert!(!constant_time_eq("secret", "secrET"));
    }
}
