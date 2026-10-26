-- The hibernation snapshot of one relay room. Written by the engine's
-- save/load hooks; the generated models.rs types its rows.

CREATE TABLE IF NOT EXISTS room_persistent_snapshot (
    room_name      VARCHAR(255) PRIMARY KEY,
    backend        VARCHAR(32) NOT NULL,
    snapshot_bytes BYTEA NOT NULL,
    entity_count   INT NOT NULL DEFAULT 0,
    saved_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
