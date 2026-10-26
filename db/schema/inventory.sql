-- The inventory tables the generated repositories model. Their queries are
-- hand-written beside the service (repositories/inventory.rs) — this file
-- exists so `flash gen` types their rows in models.rs.

CREATE TABLE IF NOT EXISTS item_definition (
    item_id     VARCHAR(128) PRIMARY KEY,
    name        VARCHAR(255) NOT NULL,
    category    VARCHAR(64) NOT NULL,
    max_stack   INT NOT NULL DEFAULT 1,
    rarity      VARCHAR(32) NOT NULL DEFAULT 'common',
    properties  JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS player_inventory_item (
    instance_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID NOT NULL,
    item_id      VARCHAR(128) NOT NULL REFERENCES item_definition(item_id),
    quantity     INT NOT NULL DEFAULT 1,
    slot_index   INT NOT NULL DEFAULT 0,
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_player_inventory_item_user ON player_inventory_item(user_id);
