-- Game maps: the versioned arena assets (`game_maps`). The reads, gates and
-- deletes are generated; the insert and activate flows stay hand-written
-- beside MapService because each retires the previously active version of the
-- same format inside one transaction — the same rule console_mutations.sql
-- states for its multi-statement writes.

-- name: ListGameMaps :many
SELECT id, version, name, format, sha256, is_active, octet_length(bytes)::BIGINT AS size_bytes, created_at
FROM game_maps WHERE game_id = $1 ORDER BY version DESC;

-- name: FindGameMapFormat :many
SELECT format FROM game_maps WHERE game_id = $1 AND version = $2;

-- The delete gate: an absent row is not found, an active one is refused.
-- name: GameMapActiveFlag :many
SELECT is_active FROM game_maps WHERE game_id = $1 AND version = $2;

-- name: DeleteGameMap :execresult
DELETE FROM game_maps WHERE game_id = $1 AND version = $2;

-- The boot fetch: the hosted game's active versions, bytes included.
-- name: LoadActiveGameMaps :many
SELECT name, format, bytes, version FROM game_maps WHERE game_id = $1 AND is_active;
