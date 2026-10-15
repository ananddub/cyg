-- Agent 3: Notifications. BaaS implementation

-- name: CreateNotification :one
INSERT INTO notification (id, user_id, subject, content, sender_id, code)
VALUES ($1, $2, $3, $4, $5, $6)
RETURNING *;

-- name: ListNotifications :many
SELECT * FROM notification
WHERE user_id = $1 AND create_time < $2
ORDER BY create_time DESC LIMIT $3;

-- name: ListNotificationsBySender :many
SELECT * FROM notification
WHERE user_id = $1 AND sender_id = $2 AND create_time < $3
ORDER BY create_time DESC LIMIT $4;

-- name: ListNotificationsByCode :many
SELECT * FROM notification
WHERE user_id = $1 AND code = $2 AND create_time < $3
ORDER BY create_time DESC LIMIT $4;

-- name: DeleteNotification :one
DELETE FROM notification WHERE user_id = $1 AND id = $2 RETURNING id;

-- name: DeleteNotifications :exec
DELETE FROM notification WHERE user_id = $1;

-- BaaS pages notifications forward from a (create_time, id) cursor, ordered
-- ASC, and asks for one row past the limit to decide whether a cursor follows.
-- The first page passes NaiveDateTime::MIN and the nil UUID, which admit
-- everything without needing a nullable comparison.
-- name: ListNotificationsPaged :many
SELECT * FROM notification
WHERE user_id = $1
  AND (create_time > $2 OR (create_time = $2 AND id > $3))
ORDER BY create_time ASC, id ASC LIMIT $4;

-- name: ListNotificationsBySenderPaged :many
SELECT * FROM notification
WHERE user_id = $1 AND sender_id = $2
  AND (create_time > $3 OR (create_time = $3 AND id > $4))
ORDER BY create_time ASC, id ASC LIMIT $5;

-- name: ListNotificationsByCodePaged :many
SELECT * FROM notification
WHERE user_id = $1 AND code = $2
  AND (create_time > $3 OR (create_time = $3 AND id > $4))
ORDER BY create_time ASC, id ASC LIMIT $5;
