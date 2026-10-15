-- Agent 3 + Agent 4: Chat channels (stream messages). BaaS implementation
-- Message delete is a soft delete: code 2 = MESSAGE_DELETED.
-- The timestamp cursor is passed by the handler as either now() or a far-future
-- constant; FlashORM cannot parse $n::timestamp casts.

-- name: InsertChannelMessage :one
INSERT INTO message (id, code, sender_id, username, stream_mode, stream_subject, stream_descriptor, stream_label, content)
VALUES ($1, 0, $2, $3, $4, $5, $6, $7, $8)
RETURNING *;

-- name: ListChannelMessages :many
SELECT * FROM message
WHERE stream_mode = $1 AND stream_subject = $2 AND stream_descriptor = $3 AND stream_label = $4
  AND create_time < $5
ORDER BY create_time DESC LIMIT $6;

-- name: ListChannelMessagesBySender :many
SELECT * FROM message
WHERE stream_mode = $1 AND stream_subject = $2 AND stream_descriptor = $3 AND stream_label = $4
  AND sender_id = $5 AND create_time < $6
ORDER BY create_time DESC LIMIT $7;

-- name: GetMessageById :one
SELECT * FROM message WHERE id = $1;

-- name: UpdateChannelMessage :one
UPDATE message SET content = $2, update_time = now()
WHERE id = $1 AND sender_id = $3
RETURNING *;

-- name: DeleteChannelMessage :one
UPDATE message SET code = 2, content = '{}'::jsonb, update_time = now()
WHERE id = $1 AND sender_id = $2
RETURNING *;

-- Message paging is a (create_time, id) cursor in the direction the client
-- asked for; the first page passes an extreme timestamp and the nil UUID so the
-- comparison admits every row.
-- name: ListChannelMessagesForward :many
SELECT * FROM message
WHERE stream_mode = $1 AND stream_subject = $2 AND stream_descriptor = $3 AND stream_label = $4
  AND (create_time > $5 OR (create_time = $5 AND id > $6))
ORDER BY create_time ASC, id ASC LIMIT $7;

-- name: ListChannelMessagesBackward :many
SELECT * FROM message
WHERE stream_mode = $1 AND stream_subject = $2 AND stream_descriptor = $3 AND stream_label = $4
  AND (create_time < $5 OR (create_time = $5 AND id < $6))
ORDER BY create_time DESC, id DESC LIMIT $7;
