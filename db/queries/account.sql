-- Agent 1: account reads and profile writes. BaaS implementation
-- GetUserById lives in auth.sql, so it is deliberately not repeated here.

-- name: UpdateAccount :exec
UPDATE users SET display_name = $2, avatar_url = $3, lang_tag = $4,
  location = $5, timezone = $6, metadata = $7, update_time = now()
WHERE id = $1;

-- name: UpdatePassword :exec
UPDATE users SET password = $2, update_time = now() WHERE id = $1;

-- name: DeleteAccount :exec
DELETE FROM users WHERE id = $1;

-- name: CreateTombstone :exec
INSERT INTO user_tombstone (user_id) VALUES ($1)
ON CONFLICT (user_id) DO NOTHING;

-- provider = '' is a raw device id; anything else is a runtime provider identity,
-- and the console-facing account shape reports the two separately.
-- name: ListUserDevices :many
SELECT id, user_id, provider FROM user_device WHERE user_id = $1;

-- name: ListDevicePushTokens :many
SELECT id, push_token_ios, push_token_android, push_token_amazon, push_token_huawei, push_token_web, provider
FROM user_device WHERE user_id = $1;

-- name: UpdateDevicePushTokens :exec
UPDATE user_device SET push_token_ios = $3, push_token_android = $4, push_token_amazon = $5,
  push_token_huawei = $6, push_token_web = $7 WHERE id = $1 AND user_id = $2;
