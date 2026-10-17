-- Storage index definitions. Hand-written statements, executed raw through sqlx by
-- `services/storage/index`: the indexed read itself is assembled per query (the
-- value keys, bounds and sorts are data, not SQL), so only the definition table's
-- fixed shapes are named here.

-- name: InsertStorageIndex :one
INSERT INTO storage_index (name, collection, key, fields, sortable_fields, max_entries, index_only, user_id)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
RETURNING name, collection, key, fields, sortable_fields, max_entries, index_only, user_id;

-- name: GetStorageIndex :one
SELECT name, collection, key, fields, sortable_fields, max_entries, index_only, user_id
FROM storage_index WHERE name = $1;

-- name: ListStorageIndexes :many
SELECT name, collection, key, fields, sortable_fields, max_entries, index_only, user_id
FROM storage_index ORDER BY name ASC;

-- name: DeleteStorageIndex :exec
DELETE FROM storage_index WHERE name = $1;
