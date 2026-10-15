-- Agent 2: Storage. BaaS implementation
-- The `key` column name is reserved by FlashORM's parser, so every projection
-- aliases it to `object_key`; binds stay positional.

-- name: GetStorageObject :one
SELECT collection, key AS object_key, user_id, version, value, read, write, create_time, update_time
FROM storage WHERE collection = $1 AND key = $2 AND user_id = $3;

-- name: GetStoragePermission :one
SELECT read, write FROM storage WHERE collection = $1 AND key = $2 AND user_id = $3;

-- name: InsertStorageObject :one
INSERT INTO storage (collection, key, user_id, value, version, read, write)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT (collection, key, user_id) DO NOTHING
RETURNING collection, key AS object_key, user_id, version, value, read, write, create_time, update_time;

-- name: UpsertStorageObject :one
INSERT INTO storage (collection, key, user_id, value, version, read, write)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT (collection, key, user_id) DO UPDATE
SET value = EXCLUDED.value, version = EXCLUDED.version,
    read = EXCLUDED.read, write = EXCLUDED.write, update_time = now()
RETURNING collection, key AS object_key, user_id, version, value, read, write, create_time, update_time;

-- name: CompareAndSwapStorageObject :one
INSERT INTO storage (collection, key, user_id, value, version, read, write)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT (collection, key, user_id) DO UPDATE
SET value = EXCLUDED.value, version = EXCLUDED.version,
    read = EXCLUDED.read, write = EXCLUDED.write, update_time = now()
WHERE storage.version = $8
RETURNING collection, key AS object_key, user_id, version, value, read, write, create_time, update_time;

-- name: DeleteStorageObject :one
DELETE FROM storage
WHERE collection = $1 AND key = $2 AND user_id = $3 AND write > 0
  AND ($4 = '' OR version = $4)
RETURNING collection, key AS object_key, user_id, version;

-- name: ListStorageObjects :many
SELECT collection, key AS object_key, user_id, version, value, read, write, create_time, update_time
FROM storage
WHERE collection = $1 AND read = 2 AND key > $2
ORDER BY key ASC, user_id ASC LIMIT $3;

-- name: ListUserStorageObjects :many
SELECT collection, key AS object_key, user_id, version, value, read, write, create_time, update_time
FROM storage
WHERE collection = $1 AND user_id = $2 AND read >= 1 AND key > $3
ORDER BY key ASC LIMIT $4;
