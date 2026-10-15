-- Agent 2: Tournaments. BaaS implementation and
-- server/leaderboard_cache.go `ListTournaments`.
--
-- A tournament is a leaderboard row whose duration is not zero. Everything
-- else a tournament route runs lives in db/manual/tournament.sql and executes
-- with raw sqlx: the paged list repeats the same columns as keyset bounds, and
-- the record write, join and delete each need rows-affected. Neither survives
-- FlashORM's parameter naming.

-- name: GetTournament :one
SELECT * FROM leaderboard WHERE id = $1 AND duration <> 0;
