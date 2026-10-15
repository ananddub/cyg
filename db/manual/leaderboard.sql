-- Agent 2: Leaderboards, hand-written half. BaaS parity:
-- server/core_leaderboard.go `LeaderboardRecordWrite`.
--
-- These upserts bind metadata as NULL to mean "no metadata supplied": the insert
-- then defaults to '{}' and the update keeps the stored metadata. FlashORM types
-- a jsonb bind as a non-null Value, so the statements are executed with raw
-- sqlx in src/v2/leaderboard/write.rs and mirrored here.
--
-- Unlike tournaments, plain leaderboard writes never enforce max_num_score:
-- BaaS only counts num_score up, and the record keeps the column default.

-- name: WriteLeaderboardRecordSet :one
INSERT INTO leaderboard_record (leaderboard_id, owner_id, username, score, subscore, metadata, expiry_time)
VALUES ($1, $2, $3, $4, $5, COALESCE($6, '{}'::JSONB), $7)
ON CONFLICT (owner_id, leaderboard_id, expiry_time)
DO UPDATE SET score = $4, subscore = $5, num_score = leaderboard_record.num_score + 1,
    metadata = COALESCE($6, leaderboard_record.metadata),
    username = COALESCE($3, leaderboard_record.username), update_time = now()
WHERE leaderboard_record.score <> $4 OR leaderboard_record.subscore <> $5
RETURNING *;

-- name: WriteLeaderboardRecordBestDesc :one
INSERT INTO leaderboard_record (leaderboard_id, owner_id, username, score, subscore, metadata, expiry_time)
VALUES ($1, $2, $3, $4, $5, COALESCE($6, '{}'::JSONB), $7)
ON CONFLICT (owner_id, leaderboard_id, expiry_time)
DO UPDATE SET score = GREATEST(leaderboard_record.score, $4), subscore = GREATEST(leaderboard_record.subscore, $5),
    num_score = leaderboard_record.num_score + 1,
    metadata = COALESCE($6, leaderboard_record.metadata),
    username = COALESCE($3, leaderboard_record.username), update_time = now()
WHERE leaderboard_record.score < $4 OR leaderboard_record.subscore < $5
RETURNING *;

-- name: WriteLeaderboardRecordBestAsc :one
INSERT INTO leaderboard_record (leaderboard_id, owner_id, username, score, subscore, metadata, expiry_time)
VALUES ($1, $2, $3, $4, $5, COALESCE($6, '{}'::JSONB), $7)
ON CONFLICT (owner_id, leaderboard_id, expiry_time)
DO UPDATE SET score = LEAST(leaderboard_record.score, $4), subscore = LEAST(leaderboard_record.subscore, $5),
    num_score = leaderboard_record.num_score + 1,
    metadata = COALESCE($6, leaderboard_record.metadata),
    username = COALESCE($3, leaderboard_record.username), update_time = now()
WHERE leaderboard_record.score > $4 OR leaderboard_record.subscore > $5
RETURNING *;

-- name: WriteLeaderboardRecordInc :one
INSERT INTO leaderboard_record (leaderboard_id, owner_id, username, score, subscore, metadata, expiry_time)
VALUES ($1, $2, $3, $4, $5, COALESCE($6, '{}'::JSONB), $7)
ON CONFLICT (owner_id, leaderboard_id, expiry_time)
DO UPDATE SET score = leaderboard_record.score + $8, subscore = leaderboard_record.subscore + $9,
    num_score = leaderboard_record.num_score + 1,
    metadata = COALESCE($6, leaderboard_record.metadata),
    username = COALESCE($3, leaderboard_record.username), update_time = now()
WHERE $8 <> 0 OR $9 <> 0
RETURNING *;

-- The decrement record is inserted at 0 and only the update path subtracts, so
-- its deltas stay at $4/$5: a gap in the parameter numbering would leave those
-- two untyped and Postgres refuses to prepare it.
-- name: WriteLeaderboardRecordDec :one
INSERT INTO leaderboard_record (leaderboard_id, owner_id, username, score, subscore, metadata, expiry_time)
VALUES ($1, $2, $3, 0, 0, COALESCE($6, '{}'::JSONB), $7)
ON CONFLICT (owner_id, leaderboard_id, expiry_time)
DO UPDATE SET score = GREATEST(leaderboard_record.score - $4, 0), subscore = GREATEST(leaderboard_record.subscore - $5, 0),
    num_score = leaderboard_record.num_score + 1,
    metadata = COALESCE($6, leaderboard_record.metadata),
    username = COALESCE($3, leaderboard_record.username), update_time = now()
WHERE $4 <> 0 OR $5 <> 0
RETURNING *;
