-- game_maps: the arena/map assets a game has uploaded, one row per version.
-- The game server loads a game's ACTIVE map at boot (over its local project
-- files), so a tenant's map rides the console instead of an SSH session.
-- Mirrored in crates/engine-api/src/services/games/schema.rs (the runtime
-- bootstrap).

CREATE TABLE IF NOT EXISTS game_maps (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id       UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    version       INT NOT NULL,
    name          VARCHAR(255) NOT NULL,
    format        VARCHAR(16) NOT NULL,
    bytes         BYTEA NOT NULL,
    sha256        VARCHAR(64) NOT NULL,
    is_active     BOOLEAN NOT NULL DEFAULT false,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT game_maps_game_version UNIQUE (game_id, version),
    CONSTRAINT game_maps_format CHECK (format IN ('oxcl', 'map2d', 'map3d', 'navmesh'))
);

CREATE INDEX IF NOT EXISTS idx_game_maps_game ON game_maps(game_id, version DESC);
