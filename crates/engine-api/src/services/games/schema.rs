use crate::db::{Datastore, DbError};

pub static DEFAULT_GAME_CONFIG_JSON: &str =
    r#"{"tick_rate": 128, "max_clients": 16, "persistent_rooms": false, "auto_close_seconds": 30}"#;

pub async fn ensure_schema(db: &Datastore) -> Result<(), DbError> {
    crate::db::schema::ensure_schema(db).await
}
