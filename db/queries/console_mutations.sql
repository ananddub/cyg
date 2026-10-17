-- Console writes: creating, editing and disabling an operator; the player-account
-- writes (ban, notes, wallet, export rows); and the catalog writes (messages,
-- settings, ACL templates, the audit filter).
--
-- `console_user.role` is deliberately never written. It is the column the ACL
-- document replaced, and the role is derived from `acl` — writing both would let
-- the two disagree, and a reader would have to know which one wins.
--
-- Two writes cannot live here: the account import runs as one transaction over six
-- statements, and the storage writes name the reserved `key` column. Both stay
-- hand-written beside the service that owns them.

-- name: CountConsoleUsers :one
SELECT COUNT(id) FROM console_user;

-- name: FindConsoleUserById :one
SELECT id, username, email, acl, metadata, create_time, update_time, disable_time, mfa_required FROM console_user WHERE id = $1;

-- name: FindConsoleUserMfa :one
SELECT id, username, email, acl, password, disable_time, mfa_secret, mfa_recovery_codes, mfa_required FROM console_user WHERE username = $1 OR email = $1;

-- name: FindConsoleUserMfaById :one
SELECT id, username, email, acl, password, disable_time, mfa_secret, mfa_recovery_codes, mfa_required FROM console_user WHERE id = $1;

-- name: UpsertConsoleUser :exec
INSERT INTO console_user (id, email, acl, username, password, mfa_required) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (id) DO UPDATE SET update_time = now(), acl = $3, username = $4, password = $5, mfa_required = $6;

-- name: InsertConsoleUser :exec
INSERT INTO console_user (id, username, email, acl, password, mfa_required) VALUES ($1, $2, $3, $4, $5, $6);

-- name: UpdateConsoleUser :execresult
UPDATE console_user SET username = $1, email = $2, acl = $3, metadata = $4, mfa_required = $5, update_time = now() WHERE id = $6;

-- name: UpdateConsoleUserPassword :exec
UPDATE console_user SET password = $1, update_time = now() WHERE id = $2;

-- name: SetConsoleUserDisabled :execresult
UPDATE console_user SET disable_time = $1, update_time = now() WHERE id = $2;

-- name: SetConsoleUserMfa :execresult
UPDATE console_user SET mfa_secret = $1, mfa_recovery_codes = $2, mfa_required = $3, update_time = now() WHERE id = $4;

-- name: ClearConsoleUserMfa :execresult
UPDATE console_user SET mfa_secret = NULL, mfa_recovery_codes = NULL, mfa_required = false, update_time = now() WHERE id = $1;

-- name: SetConsoleUserRecoveryCodes :execresult
UPDATE console_user SET mfa_recovery_codes = $1, update_time = now() WHERE id = $2;

-- name: DeleteConsoleUser :exec
DELETE FROM console_user WHERE id = $1;

-- name: SetAccountDisabled :execresult
UPDATE users SET disable_time = $1, update_time = now() WHERE id = $2;

-- name: InsertUserNote :exec
INSERT INTO users_notes (id, user_id, note, create_id) VALUES ($1, $2, $3, $4);

-- name: InsertUserNoteAnonymous :exec
INSERT INTO users_notes (id, user_id, note) VALUES ($1, $2, $3);

-- name: DeleteUserNote :execresult
DELETE FROM users_notes WHERE id = $1 AND user_id = $2;

-- name: DeleteStorageCollection :execresult
DELETE FROM storage WHERE collection = $1;

-- name: ListStorageForUser :many
SELECT collection, key AS object_key, user_id, value::text AS value, version, read, write, create_time, update_time FROM storage WHERE user_id = $1 ORDER BY collection ASC, object_key ASC;

-- name: ListUserEdges :many
SELECT destination_id, state, position, metadata FROM user_edge WHERE source_id = $1 ORDER BY state ASC, position ASC;

-- name: InsertNotification :exec
INSERT INTO notification (id, user_id, subject, content, code, sender_id) VALUES ($1, $2, $3, $4, $5, $6);

-- name: UpsertSetting :exec
INSERT INTO setting (name, value) VALUES ($1, $2) ON CONFLICT (name) DO UPDATE SET value = $2, update_time = now();

-- name: DeleteSetting :execresult
DELETE FROM setting WHERE name = $1;

-- name: InsertAclTemplate :exec
INSERT INTO console_acl_template (id, name, description, acl) VALUES ($1, $2, $3, $4);

-- name: UpdateAclTemplate :execresult
UPDATE console_acl_template SET name = $1, description = $2, acl = $3, update_time = now() WHERE id = $4;

-- name: FindAclTemplate :one
SELECT id, name, description, acl, create_time, update_time FROM console_acl_template WHERE id = $1;

-- name: DeleteAclTemplate :execresult
DELETE FROM console_acl_template WHERE id = $1;

-- name: ListAuditLogsFiltered :many
SELECT create_time, id, console_user_id, console_username, email, action, resource, message, metadata FROM console_audit_log WHERE ($1 = '' OR action = $1) AND ($2 = '' OR resource = $2) AND ($3 = '' OR console_username = $3) AND create_time >= $4 AND create_time <= $5 ORDER BY create_time DESC, id ASC LIMIT $6 OFFSET $7;

-- name: InsertAuditLog :exec
INSERT INTO console_audit_log (id, console_user_id, console_username, email, action, resource, message, metadata) VALUES ($1, $2, $3, $4, $5, $6, $7, $8);
