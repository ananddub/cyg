-- room_modules: the compiled room binaries a game has uploaded, one row per
-- version. Mirrored in crates/engine-api/src/services/games/schema.rs (the
-- runtime bootstrap).

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
