-- Agent 3: User profiles. BaaS implementation
-- FlashORM infers a column's Rust type across ALL queries, so `= ANY($1)`
-- would poison scalar users.id columns elsewhere (Vec<Uuid>). Multi-get is
-- therefore done by the handler looping over the single-row queries
-- (GetUserById in account.sql, GetUserByCustomId/Username in auth.sql).

-- name: GetUserByUsernameSingle :one
SELECT * FROM users WHERE username = $1 AND disable_time = '1970-01-01 00:00:00+00';

-- name: SearchUsersByName :many
SELECT * FROM users
WHERE username ILIKE '%' || $1 || '%' AND disable_time = '1970-01-01 00:00:00+00'
ORDER BY username ASC LIMIT $2;
