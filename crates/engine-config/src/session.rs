//! `[control.session]`: the BaaS-compatible account plane (`/v2/*`).
//!
//! Defaults mirror the own `config.yaml` so an unmodified BaaS SDK and a
//! BaaS config file behave the same here. The two signing keys are separate
//! because a session token and a refresh token are not interchangeable: which key
//! verifies a JWT is what says which one it is.

/// SurrealDB target for the account plane. An empty `database_endpoint` leaves
/// `/v2/*` mounted but answering 503, so a game that needs no accounts still
/// boots with no database at all.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// `surrealkv://data/cyg` (embedded — one process per datastore),
    /// `mem://` (tests), or `ws(s)://…` / `http(s)://…` (remote).
    pub database_endpoint: String,
    /// Namespace and database selected on that endpoint.
    pub database_ns: String,
    pub database_db: String,
    /// Root credentials, used only for remote endpoints.
    pub database_user: String,
    pub database_pass: String,
    /// SurrealKV memtable ceiling in MiB, applied when the endpoint is
    /// `surrealkv://`. A single transaction cannot exceed it. `0` keeps the
    /// engine's own host-derived size, which is unbounded on a large host.
    pub database_memtable_mib: u64,
    /// SurrealKV block cache ceiling in MiB; `0` keeps the engine's default.
    pub database_cache_mib: u64,
    /// Where uploaded blobs live: `file:data/uploads` (disk, default embedded),
    /// `memory://`, or a SurrealDB bucket backend URL (`s3:/bucket?…`). Empty
    /// picks the embedded default; a remote endpoint needs it configured.
    pub blobs: String,
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
            database_endpoint: String::new(),
            database_ns: "cyg".into(),
            database_db: "cyg".into(),
            database_user: String::new(),
            database_pass: String::new(),
            // Sized for a 500 MB tier: together they cap the engine's data
            // memory at 96 MiB, well under what the host would otherwise pick.
            database_memtable_mib: 64,
            database_cache_mib: 32,
            blobs: String::new(),
            signing_key: "defaultsigningkey".into(),
            refresh_signing_key: "defaultrefreshsigningkey".into(),
            server_key: "defaultkey".into(),
            token_expiry_secs: 60,
            refresh_expiry_secs: 3_600,
        }
    }
}
