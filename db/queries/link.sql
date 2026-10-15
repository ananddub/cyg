-- Agent 1: identity linking writes for /v2/account/link*.
-- BaaS implementation
-- The guarded UPDATEs return no row when the identity is already taken by another
-- account, which is exactly the AlreadyExists branch the handlers report; a plain
-- UPDATE would silently hand one account another account's identity.

-- name: LinkEmail :one
UPDATE users
SET email = $2, password = $3, update_time = now()
WHERE (id = $1)
AND (NOT EXISTS (SELECT id FROM users WHERE email = $2 AND NOT id = $1))
RETURNING id;

-- name: LinkCustomId :one
UPDATE users
SET custom_id = $2, update_time = now()
WHERE (id = $1)
AND (NOT EXISTS (SELECT id FROM users WHERE custom_id = $2 AND NOT id = $1))
RETURNING id;

-- name: LinkApple :one
UPDATE users
SET apple_id = $2, update_time = now()
WHERE (id = $1)
AND (NOT EXISTS (SELECT id FROM users WHERE apple_id = $2 AND NOT id = $1))
RETURNING id;

-- name: LinkGoogle :one
UPDATE users AS u
SET google_id = $2, display_name = COALESCE(NULLIF(u.display_name, ''), $3), avatar_url = COALESCE(NULLIF(u.avatar_url, ''), $4), update_time = now()
WHERE (id = $1)
AND (NOT EXISTS (SELECT id FROM users WHERE google_id = $2 AND NOT id = $1))
RETURNING id;

-- name: LinkSteam :one
UPDATE users
SET steam_id = $2, update_time = now()
WHERE (id = $1)
AND (NOT EXISTS (SELECT id FROM users WHERE steam_id = $2 AND NOT id = $1))
RETURNING id;

-- name: LinkFacebook :one
UPDATE users AS u
SET facebook_id = $2, display_name = COALESCE(NULLIF(u.display_name, ''), $3), avatar_url = COALESCE(NULLIF(u.avatar_url, ''), $4), update_time = now()
WHERE (id = $1)
AND (NOT EXISTS (SELECT id FROM users WHERE facebook_id = $2 AND NOT id = $1))
RETURNING id;

-- name: LinkGamecenter :one
UPDATE users
SET gamecenter_id = $2, update_time = now()
WHERE (id = $1)
AND (NOT EXISTS (SELECT id FROM users WHERE gamecenter_id = $2 AND NOT id = $1))
RETURNING id;

-- name: LinkFacebookInstant :one
UPDATE users
SET facebook_instant_game_id = $2, update_time = now()
WHERE (id = $1)
AND (NOT EXISTS (SELECT id FROM users WHERE facebook_instant_game_id = $2 AND NOT id = $1))
RETURNING id;

-- Provider sign-ups may carry an email the account does not have yet. Importing a
-- duplicate is a soft failure the caller swallows, so these stay unguarded.
-- name: ImportEmail :exec
UPDATE users SET email = $2 WHERE id = $1;

-- name: ImportEmailIfEmpty :exec
UPDATE users SET email = $2 WHERE id = $1 AND NULLIF(email, '') IS NULL;

-- name: CountOwnedDevice :one
SELECT COUNT(id) FROM user_device WHERE id = $1::VARCHAR AND user_id = $2 LIMIT 1;

-- The conflict clause doubles as the ownership test: a row owned by someone else
-- fails its WHERE and is reported as no row returned.
-- name: UpsertProviderIdentity :one
INSERT INTO user_device (provider, id, user_id)
VALUES ($1::VARCHAR, $2::VARCHAR, $3)
ON CONFLICT (id)
DO UPDATE SET user_id = EXCLUDED.user_id
WHERE user_device.user_id = EXCLUDED.user_id
RETURNING id;

-- name: TouchUser :one
UPDATE users SET update_time = now() WHERE id = $1
RETURNING id;
