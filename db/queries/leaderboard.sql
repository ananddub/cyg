-- Agent 2: Leaderboards. BaaS implementation
--
-- Rank and pagination both walk the (score, subscore, owner_id) tuple in the
-- board's sort order. The tuple is unique per window, so its comparison is
-- written out column by column: that keeps FlashORM's parameter type
-- inference, which a row-value constructor on the right-hand side defeats.
-- The record upserts need a nullable metadata bind, so they live in
-- db/manual/leaderboard.sql and are executed with raw sqlx.

-- name: GetLeaderboard :one
SELECT * FROM leaderboard WHERE id = $1;

-- name: DefineLeaderboard :one
INSERT INTO leaderboard (id, title, description, operator, sort_order, reset_schedule, metadata, authoritative, category, max_num_score)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
ON CONFLICT (id) DO UPDATE
SET title = EXCLUDED.title, description = EXCLUDED.description,
    operator = EXCLUDED.operator, sort_order = EXCLUDED.sort_order,
    reset_schedule = EXCLUDED.reset_schedule, metadata = EXCLUDED.metadata,
    authoritative = EXCLUDED.authoritative, category = EXCLUDED.category,
    max_num_score = EXCLUDED.max_num_score
RETURNING *;

-- name: GetLeaderboardRecord :one
SELECT * FROM leaderboard_record
WHERE leaderboard_id = $1 AND owner_id = $2 AND expiry_time = $3;

-- name: GetLeaderboardRecordRankDesc :one
SELECT COUNT(*) AS rank FROM leaderboard_record
WHERE leaderboard_id = $1 AND expiry_time = $2
  AND (score > $3 OR (score = $3 AND subscore > $4) OR (score = $3 AND subscore = $4 AND owner_id > $5));

-- name: GetLeaderboardRecordRankAsc :one
SELECT COUNT(*) AS rank FROM leaderboard_record
WHERE leaderboard_id = $1 AND expiry_time = $2
  AND (score < $3 OR (score = $3 AND subscore < $4) OR (score = $3 AND subscore = $4 AND owner_id < $5));

-- name: CountLeaderboardRecords :one
SELECT COUNT(*) AS rank_count FROM leaderboard_record WHERE leaderboard_id = $1 AND expiry_time = $2;

-- name: FirstLeaderboardRecordsAsc :many
SELECT leaderboard_id, owner_id, username, score, subscore, num_score, max_num_score,
       metadata, create_time, update_time, expiry_time
FROM leaderboard_record
WHERE leaderboard_id = $1 AND expiry_time = $2
ORDER BY score ASC, subscore ASC, owner_id ASC
LIMIT $3;

-- name: FirstLeaderboardRecordsDesc :many
SELECT leaderboard_id, owner_id, username, score, subscore, num_score, max_num_score,
       metadata, create_time, update_time, expiry_time
FROM leaderboard_record
WHERE leaderboard_id = $1 AND expiry_time = $2
ORDER BY score DESC, subscore DESC, owner_id DESC
LIMIT $3;

-- name: GreaterLeaderboardRecordsAsc :many
SELECT leaderboard_id, owner_id, username, score, subscore, num_score, max_num_score,
       metadata, create_time, update_time, expiry_time
FROM leaderboard_record
WHERE leaderboard_id = $1 AND expiry_time = $2
  AND (score > $3 OR (score = $3 AND subscore > $4) OR (score = $3 AND subscore = $4 AND owner_id > $5))
ORDER BY score ASC, subscore ASC, owner_id ASC
LIMIT $6;

-- name: LessLeaderboardRecordsDesc :many
SELECT leaderboard_id, owner_id, username, score, subscore, num_score, max_num_score,
       metadata, create_time, update_time, expiry_time
FROM leaderboard_record
WHERE leaderboard_id = $1 AND expiry_time = $2
  AND (score < $3 OR (score = $3 AND subscore < $4) OR (score = $3 AND subscore = $4 AND owner_id < $5))
ORDER BY score DESC, subscore DESC, owner_id DESC
LIMIT $6;

-- name: ListLeaderboardRecordsForOwner :many
SELECT leaderboard_id, owner_id, username, score, subscore, num_score, max_num_score,
       metadata, create_time, update_time, expiry_time
FROM leaderboard_record
WHERE leaderboard_id = $1 AND expiry_time = $2 AND owner_id = ANY($3);

-- name: DeleteLeaderboardRecord :exec
DELETE FROM leaderboard_record WHERE leaderboard_id = $1 AND owner_id = $2 AND expiry_time = $3;
