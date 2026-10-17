-- Console reads: the lists and lookups the console pages are built from.
--
-- `console_user` is always selected column by column, never `SELECT *`: the
-- password and MFA secrets must never travel into a view, and the legacy `role`
-- column is not read at all — the role comes from `acl`.
-- Because the query parser reserves the `key` column name, every storage
-- projection aliases it to `object_key`.

-- name: FindConsoleUser :one
SELECT id, username, email, acl, metadata, create_time, update_time, disable_time, mfa_required FROM console_user WHERE username = $1 OR email = $1;

-- name: ListConsoleUsers :many
SELECT id, username, email, acl, metadata, create_time, update_time, disable_time, mfa_required FROM console_user ORDER BY create_time DESC, id ASC LIMIT $1 OFFSET $2;

-- name: ListAccounts :many
SELECT * FROM users ORDER BY create_time DESC, id ASC LIMIT $1 OFFSET $2;

-- name: CountAccounts :one
SELECT count(*) FROM users;

-- name: ListWalletLedger :many
SELECT id, user_id, changeset, metadata, create_time, update_time FROM wallet_ledger WHERE user_id = $1 ORDER BY create_time DESC, id ASC LIMIT $2 OFFSET $3;

-- name: ListUserNotes :many
SELECT id, user_id, create_time, update_time, note, create_id, update_id FROM users_notes WHERE user_id = $1 ORDER BY create_time DESC;

-- name: ListLeaderboards :many
SELECT * FROM leaderboard ORDER BY create_time DESC, id ASC LIMIT $1 OFFSET $2;

-- name: ListLeaderboardsByCategory :many
SELECT * FROM leaderboard WHERE category = $1 ORDER BY create_time DESC, id ASC LIMIT $2 OFFSET $3;

-- name: ListAllLeaderboardRecords :many
SELECT leaderboard_id, owner_id, username, score, subscore, num_score, metadata, create_time, update_time, expiry_time, max_num_score FROM leaderboard_record WHERE leaderboard_id = $1 ORDER BY score DESC, subscore DESC, owner_id DESC LIMIT $2;

-- name: ListStorage :many
SELECT collection, key AS object_key, user_id, value::text AS value, version, read, write, create_time, update_time FROM storage ORDER BY update_time DESC, collection ASC, object_key ASC LIMIT $1 OFFSET $2;

-- name: ListStorageInCollection :many
SELECT collection, key AS object_key, user_id, value::text AS value, version, read, write, create_time, update_time FROM storage WHERE collection = $1 ORDER BY object_key ASC, user_id ASC LIMIT $2;

-- name: ListStorageCollections :many
SELECT DISTINCT collection FROM storage ORDER BY collection ASC;

-- name: ListSettings :many
SELECT name, value, update_time FROM setting ORDER BY name ASC;

-- name: ListAuditLogs :many
SELECT create_time, id, console_user_id, console_username, email, action, resource, message, metadata FROM console_audit_log ORDER BY create_time DESC, id ASC LIMIT $1 OFFSET $2;

-- name: ListAclTemplates :many
SELECT id, name, description, acl, create_time, update_time FROM console_acl_template ORDER BY name ASC;

-- name: ListAllPurchases :many
SELECT create_time, environment, product_id, purchase_time, raw_response, store, transaction_id, update_time, user_id, refund_time FROM purchase ORDER BY purchase_time DESC, transaction_id ASC LIMIT $1 OFFSET $2;

-- name: ListNotificationsForUser :many
SELECT id, user_id, subject, content, code, sender_id, create_time FROM notification WHERE user_id = $1 ORDER BY create_time DESC LIMIT $2;
