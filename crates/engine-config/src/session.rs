
#[derive(Debug, Clone)]
pub struct SessionConfig {

    pub database_endpoint: String,

    pub database_ns: String,
    pub database_db: String,

    pub database_user: String,
    pub database_pass: String,

    pub database_memtable_mib: u64,

    pub database_cache_mib: u64,

    pub blobs: String,

    pub signing_key: String,

    pub refresh_signing_key: String,

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
