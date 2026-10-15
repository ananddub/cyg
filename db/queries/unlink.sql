-- Agent 1: identity removal writes for /v2/account/unlink*.
-- BaaS implementation
-- Every statement carries the last-identifier guard: clearing the only sign-in path
-- would lock the account out forever, so zero rows affected is reported as
-- PermissionDenied rather than success. The guard is the other seven identity columns
-- of `users` OR any remaining `user_device` row. The caller compares the stored value
-- against the presented token first, which is the `AND col = $2` the Go version folds
-- into this same WHERE clause.

-- name: UnlinkEmail :one
UPDATE users SET email = NULL, password = NULL, update_time = now()
WHERE id = $1
AND ((apple_id IS NOT NULL OR facebook_id IS NOT NULL OR facebook_instant_game_id IS NOT NULL
  OR google_id IS NOT NULL OR gamecenter_id IS NOT NULL OR steam_id IS NOT NULL OR custom_id IS NOT NULL)
  OR EXISTS (SELECT id FROM user_device WHERE user_id = $1 LIMIT 1))
RETURNING id;

-- name: UnlinkCustomId :one
UPDATE users SET custom_id = NULL, update_time = now()
WHERE id = $1
AND ((apple_id IS NOT NULL OR facebook_id IS NOT NULL OR facebook_instant_game_id IS NOT NULL
  OR google_id IS NOT NULL OR gamecenter_id IS NOT NULL OR steam_id IS NOT NULL OR email IS NOT NULL)
  OR EXISTS (SELECT id FROM user_device WHERE user_id = $1 LIMIT 1))
RETURNING id;

-- name: UnlinkApple :one
UPDATE users SET apple_id = NULL, update_time = now()
WHERE id = $1
AND ((custom_id IS NOT NULL OR facebook_id IS NOT NULL OR facebook_instant_game_id IS NOT NULL
  OR google_id IS NOT NULL OR gamecenter_id IS NOT NULL OR steam_id IS NOT NULL OR email IS NOT NULL)
  OR EXISTS (SELECT id FROM user_device WHERE user_id = $1 LIMIT 1))
RETURNING id;

-- name: UnlinkGoogle :one
UPDATE users SET google_id = NULL, update_time = now()
WHERE id = $1
AND ((apple_id IS NOT NULL OR custom_id IS NOT NULL OR facebook_instant_game_id IS NOT NULL
  OR facebook_id IS NOT NULL OR gamecenter_id IS NOT NULL OR steam_id IS NOT NULL OR email IS NOT NULL)
  OR EXISTS (SELECT id FROM user_device WHERE user_id = $1 LIMIT 1))
RETURNING id;

-- name: UnlinkSteam :one
UPDATE users SET steam_id = NULL, update_time = now()
WHERE id = $1
AND ((apple_id IS NOT NULL OR custom_id IS NOT NULL OR facebook_id IS NOT NULL
  OR facebook_instant_game_id IS NOT NULL OR google_id IS NOT NULL OR gamecenter_id IS NOT NULL OR email IS NOT NULL)
  OR EXISTS (SELECT id FROM user_device WHERE user_id = $1 LIMIT 1))
RETURNING id;

-- name: UnlinkFacebook :one
UPDATE users SET facebook_id = NULL, update_time = now()
WHERE id = $1
AND ((apple_id IS NOT NULL OR custom_id IS NOT NULL OR facebook_instant_game_id IS NOT NULL
  OR google_id IS NOT NULL OR gamecenter_id IS NOT NULL OR steam_id IS NOT NULL OR email IS NOT NULL)
  OR EXISTS (SELECT id FROM user_device WHERE user_id = $1 LIMIT 1))
RETURNING id;

-- name: UnlinkGamecenter :one
UPDATE users SET gamecenter_id = NULL, update_time = now()
WHERE id = $1
AND ((apple_id IS NOT NULL OR custom_id IS NOT NULL OR facebook_id IS NOT NULL
  OR facebook_instant_game_id IS NOT NULL OR google_id IS NOT NULL OR steam_id IS NOT NULL OR email IS NOT NULL)
  OR EXISTS (SELECT id FROM user_device WHERE user_id = $1 LIMIT 1))
RETURNING id;

-- name: UnlinkFacebookInstant :one
UPDATE users SET facebook_instant_game_id = NULL, update_time = now()
WHERE id = $1
AND ((apple_id IS NOT NULL OR custom_id IS NOT NULL OR facebook_id IS NOT NULL
  OR google_id IS NOT NULL OR gamecenter_id IS NOT NULL OR steam_id IS NOT NULL OR email IS NOT NULL)
  OR EXISTS (SELECT id FROM user_device WHERE user_id = $1 LIMIT 1))
RETURNING id;

-- name: UnlinkDevice :one
DELETE FROM user_device WHERE id = $2::VARCHAR AND user_id = $1
AND (EXISTS (SELECT id FROM users WHERE id = $1 AND
    (apple_id IS NOT NULL OR custom_id IS NOT NULL OR facebook_id IS NOT NULL OR facebook_instant_game_id IS NOT NULL
     OR google_id IS NOT NULL OR gamecenter_id IS NOT NULL OR steam_id IS NOT NULL OR email IS NOT NULL))
  OR EXISTS (SELECT id FROM user_device WHERE user_id = $1 AND id <> $2::VARCHAR LIMIT 1))
RETURNING id;

-- name: UnlinkProvider :one
DELETE FROM user_device WHERE provider = $2::VARCHAR AND user_id = $1
AND (EXISTS (SELECT id FROM users WHERE id = $1 AND
    (apple_id IS NOT NULL OR custom_id IS NOT NULL OR facebook_id IS NOT NULL OR facebook_instant_game_id IS NOT NULL
     OR google_id IS NOT NULL OR gamecenter_id IS NOT NULL OR steam_id IS NOT NULL OR email IS NOT NULL))
  OR EXISTS (SELECT id FROM user_device WHERE user_id = $1 AND provider <> $2::VARCHAR LIMIT 1))
RETURNING id;
