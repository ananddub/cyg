-- Games tenancy: the console's game catalog and everything the module and map
-- planes resolve against it.
--
-- The games projection below is column-by-column, never `SELECT *`: `api_key`
-- is the tenant secret and must never ride a listing or a lookup. The same
-- rule the console reads follow (`console.sql`).
--
-- The versioned-asset tables (`room_modules`) follow the same read/gate/delete
-- split; their insert and activate flows stay hand-written beside WasmService
-- because each retires the previously active version inside one transaction —
-- the rule `console_mutations.sql` states for its multi-statement writes.

-- name: InsertGame :one
INSERT INTO games (game_key, name, api_key, client_key, engine_type, backend_mode, status, config)
VALUES ($1, $2, $3, $4, $5, $6, 'active', $7)
RETURNING id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at;

-- name: ListGames :many
SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at
FROM games
ORDER BY created_at DESC;

-- name: CountGames :one
SELECT COUNT(*)::BIGINT AS total FROM games;

-- name: FindGameById :many
SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at
FROM games
WHERE id = $1;

-- name: FindGameByKey :many
SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at
FROM games
WHERE game_key = $1;

-- The secret, read on its own so it can never ride along with a listing.
-- name: FindGameSecretById :many
SELECT api_key FROM games WHERE id = $1;

-- The secret as the module plane resolves it — by the key the caller presented.
-- name: FindGameSecretByKey :many
SELECT api_key FROM games WHERE game_key = $1;

-- The patch merge happens in Rust (read the row, overlay the patch, write it
-- back whole) because this generator's parameters are always non-null — there
-- is no `sqlc.narg` here, so a COALESCE patch cannot be expressed faithfully.
-- name: UpdateGame :one
UPDATE games
SET name = $2,
    engine_type = $3,
    status = $4,
    config = $5,
    backend_mode = $6,
    updated_at = NOW()
WHERE id = $1
RETURNING id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at;

-- name: ArchiveGame :execresult
UPDATE games SET status = 'archived', updated_at = NOW() WHERE id = $1;

-- ---------------------------------------------------------------- room_modules

-- `has_source` is cast to INT because the generator types unknown expression
-- columns as String, which cannot decode a Postgres boolean; 0/1 decodes
-- reliably and the service maps it back to bool.
-- name: ActiveRoomModuleVersion :many
SELECT version FROM room_modules WHERE game_id = $1 AND is_active ORDER BY version DESC LIMIT 1;

-- name: CountRoomModules :one
SELECT COUNT(*)::BIGINT AS total FROM room_modules WHERE game_id = $1;

-- name: ListRoomModules :many
SELECT id, version, filename, sha256, is_active, octet_length(binary_bytes)::BIGINT AS size_bytes,
       created_at, (source_code IS NOT NULL)::INT AS has_source
FROM room_modules WHERE game_id = $1 ORDER BY version DESC;

-- name: FindRoomModuleBytes :many
SELECT filename, binary_bytes FROM room_modules WHERE game_id = $1 AND version = $2;

-- The insert mints the next version and retires the previously active module
-- in one statement — see the header note.
-- The delete gate: an absent row is not found, an active one is refused.
-- name: RoomModuleActiveFlag :many
SELECT is_active FROM room_modules WHERE game_id = $1 AND version = $2;

-- name: DeleteRoomModule :execresult
DELETE FROM room_modules WHERE game_id = $1 AND version = $2;
