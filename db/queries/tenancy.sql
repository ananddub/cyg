-- The tenant this process hosts: identity resolution, the legacy-row backfill
-- and the per-game page reads the dashboard tenancy views serve.
--
-- Every page read scopes by the hosted game through the same shape: the game
-- id arrives as a nullable uuid, `NULL` meaning "this process hosts no
-- registered game", in which case the page is empty rather than global. The
-- generator types the parameter `Option<Uuid>` from the cast, so one generated
-- method serves both cases.

-- name: TenancyGameById :many
SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at
FROM games WHERE id = $1;

-- name: TenancyGameByKey :many
SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at
FROM games WHERE game_key = $1;

-- The sole-active fallback: two rows come back when the database is
-- multi-tenant, which is exactly the ambiguity the caller must refuse — so the
-- query deliberately does not LIMIT to one.
-- name: TenancySoleActiveGames :many
SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at
FROM games WHERE status = 'active' ORDER BY created_at ASC LIMIT 2;

-- The backfill: idempotent claim of rows that predate the game_id column.
-- name: BackfillUsersGame :execresult
UPDATE users SET game_id = $1 WHERE game_id IS NULL;

-- name: BackfillLeaderboardGame :execresult
UPDATE leaderboard SET game_id = $1 WHERE game_id IS NULL;

-- Stamp one account (the hosted tenant on create; a named tenant on import).
-- name: StampUserGame :execresult
UPDATE users SET game_id = $1 WHERE id = $2 AND game_id IS NULL;

-- ---------------------------------------------------------------- page reads

-- name: TenancyAccountsPage :many
SELECT * FROM users WHERE ($1::uuid IS NULL OR game_id = $1::uuid) ORDER BY create_time DESC, id ASC LIMIT $2 OFFSET $3;

-- name: TenancyGroupsPage :many
SELECT * FROM groups WHERE disable_time = '1970-01-01 00:00:00+00' AND ($1::uuid IS NULL OR creator_id IN (SELECT id FROM users WHERE game_id = $1::uuid)) ORDER BY update_time ASC, edge_count ASC, id ASC LIMIT $2 OFFSET $3;

-- name: TenancyBoardsPage :many
SELECT * FROM leaderboard WHERE category = $1 AND ($2::uuid IS NULL OR game_id = $2::uuid) ORDER BY create_time DESC, id ASC LIMIT $3 OFFSET $4;

-- name: TenancyStoragePage :many
SELECT collection, key AS object_key, user_id, value::text AS value, version, read, write, create_time, update_time FROM storage WHERE ($1::uuid IS NULL OR user_id IN (SELECT id FROM users WHERE game_id = $1::uuid)) ORDER BY update_time DESC, collection ASC, object_key ASC LIMIT $2 OFFSET $3;

-- name: TenancyStorageCollections :many
SELECT DISTINCT collection FROM storage WHERE ($1::uuid IS NULL OR user_id IN (SELECT id FROM users WHERE game_id = $1::uuid)) ORDER BY collection ASC;

-- name: TenancyStorageCollection :many
SELECT collection, key AS object_key, user_id, value::text AS value, version, read, write, create_time, update_time FROM storage WHERE collection = $1 AND ($2::uuid IS NULL OR user_id IN (SELECT id FROM users WHERE game_id = $2::uuid)) ORDER BY object_key ASC, user_id ASC LIMIT $3;

-- name: TenancyPurchasesPage :many
SELECT * FROM purchase WHERE ($1::uuid IS NULL OR user_id IN (SELECT id FROM users WHERE game_id = $1::uuid)) ORDER BY purchase_time DESC, transaction_id ASC LIMIT $2 OFFSET $3;

-- name: TenancySubscriptionsPage :many
SELECT * FROM subscription WHERE ($1::uuid IS NULL OR user_id IN (SELECT id FROM users WHERE game_id = $1::uuid)) ORDER BY purchase_time DESC, original_transaction_id ASC LIMIT $2 OFFSET $3;
