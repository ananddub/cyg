use std::fmt;

#[derive(Debug, Clone)]
pub enum DbError {
    Connection(String),
    Query(String),
    Deserialize(String),
    Unique(String),
    NotFound,
}

impl DbError {
    pub fn unique_index(&self) -> Option<&str> {
        match self {
            DbError::Unique(index) => Some(index),
            _ => None,
        }
    }

    pub fn is_unique(&self) -> bool {
        matches!(self, DbError::Unique(_))
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Connection(message) => write!(f, "database connection: {message}"),
            DbError::Query(message) => write!(f, "query: {message}"),
            DbError::Deserialize(message) => write!(f, "deserialize: {message}"),
            DbError::Unique(index) => {
                write!(f, "unique index `{index}` already contains this value")
            }
            DbError::NotFound => write!(f, "record not found"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<surrealdb::Error> for DbError {
    fn from(err: surrealdb::Error) -> Self {
        let message = err.to_string();
        if let Some(index) = unique_index(&message).or_else(|| record_key(&message)) {
            return DbError::Unique(index);
        }
        if err.is_connection() {
            DbError::Connection(message)
        } else {
            DbError::Query(message)
        }
    }
}

impl From<serde_json::Error> for DbError {
    fn from(err: serde_json::Error) -> Self {
        DbError::Deserialize(err.to_string())
    }
}

fn unique_index(message: &str) -> Option<String> {
    let at = message.find("already contains")?;
    backticked_before(message, at)
}

fn record_key(message: &str) -> Option<String> {
    let at = message.find("already exists")?;
    let record = backticked_before(message, at)?;
    let (table, _) = record.split_once(':')?;
    Some(format!("{table}_pkey"))
}

fn backticked_before(message: &str, at: usize) -> Option<String> {
    let head = &message[..at];
    let open = head.find('`')?;
    let close = head.rfind('`')?;
    (close > open).then(|| head[open + 1..close].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_index_name() {
        let message =
            "Database index `users_username_unique` already contains 'a', with record `users:1`";
        assert_eq!(
            unique_index(message).as_deref(),
            Some("users_username_unique")
        );
    }

    #[test]
    fn ignores_other_errors() {
        assert_eq!(unique_index("Database index `x` does not exist"), None);
    }

    #[test]
    fn reads_the_record_key_clash() {
        let message =
            "Database record `users:u'863c407b-0bd8-4c6a-91d3-a315faf53424'` already exists";
        assert_eq!(record_key(message).as_deref(), Some("users_pkey"));
    }

    #[test]
    fn reads_a_text_key_clash_through_nested_quotes() {
        let message = "Database record `user_device:`e2e-second-device-970203d4`` already exists";
        assert_eq!(record_key(message).as_deref(), Some("user_device_pkey"));
    }
}
