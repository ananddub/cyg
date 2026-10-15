-- Agent 1: identity lookups and find-or-create writes for /v2/account/authenticate/*.
-- BaaS implementation
-- FlashORM maps an INSERT column list onto positional placeholders one-to-one, so the
-- timestamp columns are left out (both default to now()) and no function call appears
-- inside VALUES. Each statement lists only the columns its provider owns, because the
-- nullable UNIQUE columns must stay NULL: an empty string would collide with another
-- account's empty string and defeat the in-use checks that link relies on.

-- name: GetUserById :one
SELECT * FROM users WHERE id = $1 LIMIT 1;

-- name: GetUserByEmail :one
SELECT * FROM users WHERE email = $1 LIMIT 1;

-- name: GetUserByUsername :one
SELECT * FROM users WHERE username = $1 LIMIT 1;

-- name: GetUserByCustomId :one
SELECT * FROM users WHERE custom_id = $1 LIMIT 1;

-- name: GetUserByFacebookId :one
SELECT * FROM users WHERE facebook_id = $1 LIMIT 1;

-- name: GetUserByGoogleId :one
SELECT * FROM users WHERE google_id = $1 LIMIT 1;

-- name: GetUserByAppleId :one
SELECT * FROM users WHERE apple_id = $1 LIMIT 1;

-- name: GetUserBySteamId :one
SELECT * FROM users WHERE steam_id = $1 LIMIT 1;

-- name: GetUserByGamecenterId :one
SELECT * FROM users WHERE gamecenter_id = $1 LIMIT 1;

-- name: GetUserByFacebookInstantGameId :one
SELECT * FROM users WHERE facebook_instant_game_id = $1 LIMIT 1;

-- `user_device.id` is the primary key, so this one row answers both questions a caller
-- has: which account owns the id, and whether it was claimed as a raw device id (empty
-- provider) or by a runtime provider identity.
-- name: GetDeviceById :one
SELECT id, user_id, provider FROM user_device WHERE id = $1::VARCHAR LIMIT 1;

-- name: CreateUserBare :one
INSERT INTO users (id, username)
VALUES ($1, $2)
RETURNING *;

-- name: CreateUserWithCustomId :one
INSERT INTO users (id, username, custom_id)
VALUES ($1, $2, $3)
RETURNING *;

-- name: CreateUserWithEmail :one
INSERT INTO users (id, username, email, password)
VALUES ($1, $2, $3, $4)
RETURNING *;

-- name: CreateUserWithApple :one
INSERT INTO users (id, username, apple_id)
VALUES ($1, $2, $3)
RETURNING *;

-- name: CreateUserWithFacebook :one
INSERT INTO users (id, username, display_name, avatar_url, facebook_id)
VALUES ($1, $2, $3, $4, $5)
RETURNING *;

-- name: CreateUserWithFacebookInstant :one
INSERT INTO users (id, username, facebook_instant_game_id)
VALUES ($1, $2, $3)
RETURNING *;

-- name: CreateUserWithGamecenter :one
INSERT INTO users (id, username, gamecenter_id)
VALUES ($1, $2, $3)
RETURNING *;

-- name: CreateUserWithGoogle :one
INSERT INTO users (id, username, google_id, display_name, avatar_url)
VALUES ($1, $2, $3, $4, $5)
RETURNING *;

-- name: CreateUserWithSteam :one
INSERT INTO users (id, username, steam_id)
VALUES ($1, $2, $3)
RETURNING *;

-- BaaS wraps the user and device inserts in one transaction. Through a pool-only API
-- the same outcome comes from ordering: create the user, then let the device primary key
-- arbitrate the claim, and delete the user again when it says somebody won first. The
-- foreign key cascades, so the compensation cannot strand a device row.
-- name: InsertDevice :one
INSERT INTO user_device (id, user_id)
VALUES ($1, $2)
RETURNING id;

-- name: InsertProviderDevice :one
INSERT INTO user_device (provider, id, user_id)
VALUES ($1, $2, $3)
RETURNING id;

-- name: DeleteUser :exec
DELETE FROM users WHERE id = $1;
