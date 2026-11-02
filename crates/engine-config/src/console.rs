
#[derive(Debug, Clone)]
pub struct ConsoleConfig {

    pub username: String,

    pub password: String,

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
