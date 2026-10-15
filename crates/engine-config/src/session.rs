//! `[control.session]`: the BaaS-compatible account plane (`/v2/*`).
//!
//! Defaults mirror the own `config.yaml` so an unmodified BaaS SDK and a
//! BaaS config file behave the same here. The two signing keys are separate
//! because a session token and a refresh token are not interchangeable: which key
//! verifies a JWT is what says which one it is.

/// Postgres DSN plus pool ceiling for the account plane. An empty
/// `database_url` leaves `/v2/*` mounted but answering 503, so a game that
/// needs no accounts still boots with no database at all.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub database_url: String,
    pub max_db_connections: u32,
    /// HS256 key for session tokens. `OX_SIGNING_KEY` overrides it.
    pub signing_key: String,
    /// HS256 key for refresh tokens. `OX_REFRESH_SIGNING_KEY` overrides it.
    pub refresh_signing_key: String,
    /// The `Basic base64("<server_key>:")` credential that `Authenticate*` and
    /// `SessionRefresh` demand. `OX_SERVER_KEY` overrides it.
    pub server_key: String,
    pub token_expiry_secs: i64,
    pub refresh_expiry_secs: i64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            database_url: String::new(),
            max_db_connections: 10,
            signing_key: "defaultsigningkey".into(),
            refresh_signing_key: "defaultrefreshsigningkey".into(),
            server_key: "defaultkey".into(),
            token_expiry_secs: 60,
            refresh_expiry_secs: 3_600,
        }
    }
}
