//! `[control.console]`: the admin console's own credential, separate from any
//! player session.
//!
//! The console is not a player: its token is signed with its own key and is
//! never accepted by `/v2` (nor the reverse), so leaking one never grants the
//! other. `username`/`password` are a bootstrap superadmin for a fresh database;
//! rows in `console_user` are checked first once they exist.

/// The console's signing key, lifetime and bootstrap credential.
#[derive(Debug, Clone)]
pub struct ConsoleConfig {
    /// Bootstrap superadmin username. `OX_CONSOLE_USERNAME` overrides it.
    pub username: String,
    /// Bootstrap superadmin password. `OX_CONSOLE_PASSWORD` overrides it.
    pub password: String,
    /// HS256 key for console tokens. `OX_CONSOLE_SIGNING_KEY` overrides it.
    pub signing_key: String,
    pub token_expiry_secs: i64,
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            username: "admin".into(),
            password: "password".into(),
            signing_key: "defaultconsolesigningkey".into(),
            token_expiry_secs: 3600,
        }
    }
}
