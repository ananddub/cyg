-- Multi-game tenancy tables, mirrored in init-oriole.sql and in
-- crates/engine-api/src/services/games/schema.rs (the runtime bootstrap).

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
);

CREATE INDEX IF NOT EXISTS idx_games_key ON games(game_key);

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
);

CREATE INDEX IF NOT EXISTS idx_room_modules_game ON room_modules(game_id, version DESC);

-- The tenant of the player rows a game's process writes. `users` is the root:
-- accounts, storage, purchases and notifications all reach a tenant through
-- their owner. `leaderboard` carries its own tenant because boards are
-- server-authored and have no player owner.
ALTER TABLE users ADD COLUMN IF NOT EXISTS game_id UUID REFERENCES games(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_users_game ON users(game_id);

ALTER TABLE leaderboard ADD COLUMN IF NOT EXISTS game_id UUID REFERENCES games(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_leaderboard_game ON leaderboard(game_id);
