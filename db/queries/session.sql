-- Agent 1: session lifecycle. BaaS has no session table — sessions are JWTs, and
-- revocation is carried by two mechanisms with different lifetimes:
--   * the in-memory session cache, which blacklists single token ids on logout
--     (server/core_session.go and it needs no SQL), and
--   * this tombstone row, which every authenticated call checks so that a deleted
--     account cannot be resurrected by a token that outlived it.
-- BaaS implementation, server/core_account.go:847

-- name: GetUserTombstone :one
SELECT user_id, create_time FROM user_tombstone WHERE user_id = $1;

-- name: DeleteUserTombstone :exec
DELETE FROM user_tombstone WHERE user_id = $1;
