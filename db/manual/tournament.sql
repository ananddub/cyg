-- Agent 2: Tournaments, hand-written half. BaaS parity:
-- server/core_tournament.go `TournamentRecordWrite`, `TournamentJoin`,
-- `TournamentRecordDelete` and leaderboard_cache.go `ListTournaments`.
--
-- These are executed with raw sqlx in src/v2/tournament and mirrored here.
-- Each one either needs a rows-affected count (a join that changed nothing, a
-- size counter that refuses to pass max_size) or repeats a column as a keyset
-- bound, and FlashORM names parameters after the column they meet, so both
-- shapes collapse into one field.
--
-- The record write's binds are the same ten BaaS builds: $1 board, $2 owner,
-- $3 username, $4 expiry, $5/$6 the deltas the operator applies, $7 metadata,
-- $8 the board's max_num_score, and $9/$10 the absolute scores a fresh row
-- starts from, which a decrement sets to zero.

-- name: ListTournamentsPage :many
-- $1 category start, $2 category end, $3 earliest start, $4 end mode,
-- $5 now, $6 explicit end bound, $7 has cursor, $8-$11 the cursor row's own
-- ordering tuple, $12 the page limit plus one.
SELECT * FROM leaderboard
WHERE duration <> 0
  AND category >= $1 AND category <= $2
  AND start_time >= $3
  AND (
    ($4 = 0 AND (end_time = 'epoch'::timestamptz OR end_time >= $5))
    OR ($4 = 1 AND end_time = 'epoch'::timestamptz)
    OR ($4 = 2 AND end_time <> 'epoch'::timestamptz AND end_time <= $6)
  )
  AND (
    NOT $7
    OR start_time > $8
    OR (start_time = $8 AND end_time < $9)
    OR (start_time = $8 AND end_time = $9 AND category > $10)
    OR (start_time = $8 AND end_time = $9 AND category = $10 AND id > $11)
  )
ORDER BY start_time ASC, end_time DESC, category ASC, id ASC
LIMIT $12;

-- name: JoinTournamentRecord :execrows
-- A join holds the entrant's place with a zero-score row; joining twice is a
-- no-op the caller is not told about.
INSERT INTO leaderboard_record
    (leaderboard_id, owner_id, expiry_time, username, num_score, max_num_score)
VALUES ($1, $2, $3, $4, 0, $5)
ON CONFLICT (owner_id, leaderboard_id, expiry_time) DO NOTHING;

-- name: IncreaseTournamentSize :execrows
UPDATE leaderboard SET size = size + 1 WHERE id = $1 AND size < max_size;

-- name: ReleaseTournamentSize :execrows
UPDATE leaderboard SET size = size - 1 WHERE id = $1;

-- name: HasTournamentRecord :one
SELECT 1 AS exists FROM leaderboard_record
WHERE leaderboard_id = $1 AND owner_id = $2 AND expiry_time = $3;

-- name: AcceptTournamentRecord :one
-- The size counter only moves for a row this write created, which is the one
-- case num_score comes back at its insert default.
INSERT INTO leaderboard_record
    (leaderboard_id, owner_id, username, score, subscore, metadata, expiry_time, max_num_score)
VALUES ($1, $2, $3, $9, $10, COALESCE($7, '{}'::JSONB), $4, $8)
ON CONFLICT (owner_id, leaderboard_id, expiry_time)
DO UPDATE SET %OP%, num_score = leaderboard_record.num_score + 1,
    metadata = COALESCE($7, leaderboard_record.metadata),
    username = COALESCE($3, leaderboard_record.username),
    update_time = now()
RETURNING num_score, max_num_score;

-- name: AcceptJoinedTournamentRecord :exec
-- A board that requires a join has no row to insert: the attempt counter is the
-- stored max_num_score, and the WHERE filter simply declines the write.
UPDATE leaderboard_record
SET %OP%, num_score = leaderboard_record.num_score + 1,
    metadata = COALESCE($7, leaderboard_record.metadata),
    username = COALESCE($3, leaderboard_record.username),
    update_time = now()
WHERE leaderboard_id = $1 AND owner_id = $2 AND expiry_time = $4
  AND (max_num_score = 0 OR num_score < max_num_score);

-- name: DeleteTournamentRecord :execrows
DELETE FROM leaderboard_record
WHERE leaderboard_id = $1 AND owner_id = $2 AND expiry_time = $3;
