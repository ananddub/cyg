-- games: the registered tenants this deployment hosts. Mirrored in
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
