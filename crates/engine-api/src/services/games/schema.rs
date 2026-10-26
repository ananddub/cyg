//! The runtime bootstrap of the multi-game tenancy tables.
//!
//! `db/manual/games.sql` is the committed source of truth; this restates the
//! same statements one at a time so a plane that boots against an empty database
//! still has `games` and `room_modules` to write into.
//! Every statement is `IF NOT EXISTS`, so running this over a database that already
//! has the tables changes nothing.

use sqlx::PgPool;

/// A registered game: its tenant identity (`game_key`), the secret the module plane
/// authenticates with, and the runtime configuration it is played with.
pub const GAMES_TABLE_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS games (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_key      VARCHAR(64) UNIQUE NOT NULL,
    name          VARCHAR(128) NOT NULL,
    api_key       VARCHAR(128) NOT NULL,
    client_key    VARCHAR(128) NOT NULL,
    engine_type   VARCHAR(32) NOT NULL DEFAULT 'unity',
    backend_mode  VARCHAR(32) NOT NULL DEFAULT 'relay',
    status        VARCHAR(32) NOT NULL DEFAULT 'active',
    config        JSONB NOT NULL DEFAULT '{"tick_rate": 128, "max_clients": 16, "persistent_rooms": false, "auto_close_seconds": 30}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
"#;

/// The tenant lookup every authenticated module call performs.
pub const GAMES_KEY_INDEX_DDL: &str = "CREATE INDEX IF NOT EXISTS idx_games_key ON games(game_key)";

/// An uploaded room module, versioned per game. `binary_bytes` is the WASM itself;
/// `source_code` is kept only when the uploader sent it.
pub const ROOM_MODULES_TABLE_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS room_modules (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id       UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    version       INT NOT NULL,
    filename      VARCHAR(255) NOT NULL,
    binary_bytes  BYTEA NOT NULL,
    source_code   TEXT,
    sha256        VARCHAR(64) NOT NULL,
    is_active     BOOLEAN NOT NULL DEFAULT false,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT room_modules_game_version UNIQUE (game_id, version)
)
"#;

/// The "latest version of this game's module" read, which is what the matchmake plane
/// resolves before it boots a room.
pub const ROOM_MODULES_INDEX_DDL: &str =
    "CREATE INDEX IF NOT EXISTS idx_room_modules_game ON room_modules(game_id, version DESC)";

/// The `(game_id, version)` uniqueness rule as a standalone statement, for a database
/// whose `room_modules` predates it. Postgres has no `ADD CONSTRAINT IF NOT EXISTS`,
/// so the rule is asserted and a name clash is swallowed — the effect is exactly
/// `IF NOT EXISTS`.
pub const ROOM_MODULES_VERSION_UQ_DDL: &str = r#"
DO $$ BEGIN
    ALTER TABLE room_modules
        ADD CONSTRAINT room_modules_game_version UNIQUE (game_id, version);
EXCEPTION
    WHEN duplicate_table THEN NULL;
    WHEN duplicate_object THEN NULL;
END $$
"#;

/// The document `games.config` defaults to, restated as one value so a create that
/// names no config and the column's own default agree.
pub static DEFAULT_GAME_CONFIG_JSON: &str =
    r#"{"tick_rate": 128, "max_clients": 16, "persistent_rooms": false, "auto_close_seconds": 30}"#;

/// The tenant of rows that belong to a player. Legacy rows are NULL until the
/// first process resolves its game and claims them (`tenant::backfill`).
pub const USERS_GAME_COLUMN_DDL: &str = r#"
ALTER TABLE users ADD COLUMN IF NOT EXISTS game_id UUID REFERENCES games(id) ON DELETE CASCADE
"#;

pub const USERS_GAME_INDEX_DDL: &str =
    "CREATE INDEX IF NOT EXISTS idx_users_game ON users(game_id)";

/// A board (and, through `category = 1`, a tournament) belongs to the game whose
/// console created it. Records already inherit the board's tenant.
pub const LEADERBOARD_GAME_COLUMN_DDL: &str = r#"
ALTER TABLE leaderboard ADD COLUMN IF NOT EXISTS game_id UUID REFERENCES games(id) ON DELETE CASCADE
"#;

pub const LEADERBOARD_GAME_INDEX_DDL: &str =
    "CREATE INDEX IF NOT EXISTS idx_leaderboard_game ON leaderboard(game_id)";

/// Create the tenancy tables if they are missing. Order matters only in that a game
/// must exist before a module can point at one.
pub async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    for ddl in [
        GAMES_TABLE_DDL,
        GAMES_KEY_INDEX_DDL,
        ROOM_MODULES_TABLE_DDL,
        ROOM_MODULES_INDEX_DDL,
        ROOM_MODULES_VERSION_UQ_DDL,
    ] {
        sqlx::query(ddl).execute(pool).await?;
    }
    Ok(())
}

/// Add the tenant columns to the player tables.
///
/// The player tables are created by the BaaS schema, not by this bootstrap, so a
/// plane pointed at a database that has not imported that schema yet must still
/// boot. Each step is therefore independent and a missing table is logged rather
/// than fatal — the same tolerance `ensure_schema` has for an unreachable
/// database.
pub async fn ensure_tenant_schema(pool: &PgPool) {
    for ddl in [
        USERS_GAME_COLUMN_DDL,
        USERS_GAME_INDEX_DDL,
        LEADERBOARD_GAME_COLUMN_DDL,
        LEADERBOARD_GAME_INDEX_DDL,
    ] {
        if let Err(err) = sqlx::query(ddl).execute(pool).await {
            tracing::info!("games: tenant schema step skipped: {err}");
        }
    }
}
