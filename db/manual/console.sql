-- Agent: Console. The two statements FlashORM cannot generate, mirrored in
-- `crates/engine-api/src/services/console/statements.rs`.
--
-- The import is one transaction over six statements, so it cannot be a single
-- generated method. The storage DELETE names the reserved `key` column, which the
-- parser rejects outside a projection alias.

-- name: ImportAccountBundle :exec
INSERT INTO users (id, username, display_name, avatar_url, lang_tag, location, timezone, metadata, wallet, email, facebook_id, google_id, gamecenter_id, steam_id, custom_id, edge_count, create_time, update_time, verify_time, disable_time, facebook_instant_game_id, apple_id)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22);
INSERT INTO user_device (id, user_id, provider) VALUES ($1, $2, $3);
INSERT INTO users_notes (id, user_id, note, create_time, update_time) VALUES ($1, $2, $3, $4, $5);
INSERT INTO wallet_ledger (id, user_id, changeset, metadata, create_time, update_time) VALUES ($1, $2, $3, $4, $5, $6);
INSERT INTO storage (collection, key, user_id, value, version, read, write, create_time, update_time) VALUES ($1, $2, $3, $4::jsonb, $5, $6, $7, $8, $9);
INSERT INTO user_edge (source_id, destination_id, state, position, metadata) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (source_id, state, position) DO NOTHING;

-- name: DeleteStorageObject :exec
DELETE FROM storage WHERE collection = $1 AND key = $2 AND user_id = $3;
