use super::*;

impl From<ListGamesRow> for GameRow {
    fn from(row: ListGamesRow) -> Self {
        GameRow {
            id: row.id,
            game_key: row.game_key,
            name: row.name,
            client_key: row.client_key,
            engine_type: row.engine_type,
            backend_mode: row.backend_mode,
            status: row.status,
            config: row.config,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
pub struct GamesService;
#[singleton]
impl GamesService {
    fn new() -> Self {
        Self
    }
}
pub(super) const KEY_ATTEMPTS: usize = 4;
pub(super) const DEFAULT_ROOM_NAME: &str = "cyg_room";
pub(super) const SERVER_WS_PATH: &str = "/ws";
