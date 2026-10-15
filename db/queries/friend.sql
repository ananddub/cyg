-- Agent 3: Friends (user_edge bidirectional graph). BaaS implementation
-- States: 0=Friends, 1=InviteSent, 2=InviteReceived, 3=Blocked

-- name: AddFriendEdge :exec
INSERT INTO user_edge (source_id, destination_id, state, position, metadata)
VALUES ($1, $2, $3, $4, '{}'::jsonb)
ON CONFLICT (source_id, destination_id) DO UPDATE SET state = $3, update_time = now();

-- name: DeleteFriendEdges :exec
DELETE FROM user_edge
WHERE (source_id = $1 AND destination_id = $2) OR (source_id = $2 AND destination_id = $1);

-- name: BlockUserEdges :exec
INSERT INTO user_edge (source_id, destination_id, state, position, metadata)
VALUES ($1, $2, 3, $3, '{}'::jsonb)
ON CONFLICT (source_id, destination_id) DO UPDATE SET state = 3, update_time = now();

-- Unblocking destroys the block, in both directions if both sides blocked the
-- other; it never resurrects a friendship (core_friend.go deletes state 3).
-- name: DeleteBlockedEdges :exec
DELETE FROM user_edge
WHERE ((source_id = $1 AND destination_id = $2) OR (source_id = $2 AND destination_id = $1))
  AND state = 3;

-- name: ListUserEdgesByState :many
SELECT CAST(u.id AS UUID) AS id, u.username, u.display_name, u.avatar_url, u.metadata, u.lang_tag, u.location,
       u.timezone, u.create_time, u.update_time, ue.state, ue.update_time AS edge_update_time
FROM user_edge ue
JOIN users u ON u.id = ue.destination_id
WHERE ue.source_id = $1 AND ue.state = $2
ORDER BY u.username ASC LIMIT $3;

-- name: GetFriendEdgeState :one
SELECT state FROM user_edge WHERE source_id = $1 AND destination_id = $2;

-- name: IncrementUserEdgeCount :exec
UPDATE users SET edge_count = edge_count + $2, update_time = now() WHERE id = $1;

-- Friends of friends are the candidates my friends point at, minus myself and
-- minus the people I already list (core_friend.go's `!= ALL(friends)`). The
-- pair (referrer, candidate) is the sort key and the cursor, exactly as in
-- BaaS, so a nil uuid means "from the beginning".
-- name: ListFriendsOfFriends :many
SELECT ue.source_id AS referrer, ue.destination_id AS id, u.username, u.display_name, u.avatar_url, u.metadata, u.create_time, u.update_time
FROM user_edge a
JOIN user_edge ue ON ue.source_id = a.destination_id AND ue.state = 0
JOIN users u ON u.id = ue.destination_id
WHERE a.source_id = $1 AND a.state = 0
  AND ue.destination_id <> $1
  AND NOT EXISTS (
    SELECT 1 FROM user_edge f WHERE f.source_id = $1 AND f.state = 0 AND f.destination_id = ue.destination_id
  )
  AND (ue.source_id > $2 OR (ue.source_id = $2 AND ue.destination_id > $3))
ORDER BY ue.source_id ASC, ue.destination_id ASC LIMIT $4;

-- name: UpdateFriendEdgeMetadata :exec
UPDATE user_edge SET metadata = $3, update_time = now() WHERE source_id = $1 AND destination_id = $2;

-- An invite is a pair of edges with opposite states. The handler inserts each
-- direction with this statement: FlashORM types a placeholder that only ever
-- appears in a VALUES list as `String`, so the pair is two calls, not one
-- multi-row INSERT (core_friend.go inserts with ON CONFLICT DO NOTHING).
-- The invite carries the sender's metadata on the forward edge only.
-- name: InsertFriendEdge :exec
INSERT INTO user_edge (source_id, destination_id, state, position, metadata)
VALUES ($1, $2, $3, $4, $5::JSONB)
ON CONFLICT (source_id, destination_id) DO NOTHING;

-- Accepting an invite promotes BOTH edges to friends in one statement.
-- name: AcceptFriendEdges :exec
UPDATE user_edge SET state = 0, update_time = now()
WHERE (source_id = $1 AND destination_id = $2) OR (source_id = $2 AND destination_id = $1);

-- Blocking keeps the blocker's edge (as state 3) and drops the reverse edge,
-- but never a reverse edge that is itself a block (core_friend.go:775).
-- name: DeleteReverseEdgeNotBlocked :exec
DELETE FROM user_edge WHERE source_id = $2 AND destination_id = $1 AND state <> 3;

-- Unfriend / cancel-invite removes both edges but never a block: a block is
-- only lifted by an explicit unblock (core_friend.go deletes with the state
-- filter friends|invite_sent|invite_received).
-- name: DeleteUnblockedEdges :exec
DELETE FROM user_edge
WHERE ((source_id = $1 AND destination_id = $2) OR (source_id = $2 AND destination_id = $1))
  AND state <> 3;

-- BaaS pages edges by (state, position). The cursor is the pair the client
-- echoed back; state -1 / position -1 means "from the beginning", because every
-- real state is >= 0.
-- name: ListUserEdgesPaged :many
SELECT CAST(u.id AS UUID) AS id, u.username, u.display_name, u.avatar_url, u.metadata, u.lang_tag,
       u.location, u.timezone, u.create_time, u.update_time,
       ue.state, ue.position AS edge_position, ue.update_time AS edge_update_time,
       ue.metadata AS edge_metadata
FROM user_edge ue
JOIN users u ON u.id = ue.destination_id
WHERE ue.source_id = $1
  AND (ue.state > $2 OR (ue.state = $2 AND ue.position > $3))
ORDER BY ue.state ASC, ue.position ASC LIMIT $4;

-- Same row shape with the state filter the client asked for (NULL-free sentinel:
-- the handler picks this query only when a state was given).
-- name: ListUserEdgesByStatePaged :many
SELECT CAST(u.id AS UUID) AS id, u.username, u.display_name, u.avatar_url, u.metadata, u.lang_tag,
       u.location, u.timezone, u.create_time, u.update_time,
       ue.state, ue.position AS edge_position, ue.update_time AS edge_update_time,
       ue.metadata AS edge_metadata
FROM user_edge ue
JOIN users u ON u.id = ue.destination_id
WHERE ue.source_id = $1 AND ue.state = $2 AND ue.position > $3
ORDER BY ue.position ASC LIMIT $4;

-- ── the exact friend state machine (core_friend.go) ────────────────────
-- Adding a friend is an INVITE: `me -> them` becomes state 1 and the reverse
-- becomes state 2. Only when the other side adds us back do both edges promote
-- to 0. These three statements are the, self-guarded so the handler never
-- has to read rows-affected (which FlashORM's :exec does not return).

-- Accepting promotes both directions of a pending invite (one side 1, the
-- other 2) to friends. Written symmetrically on purpose: FlashORM renumbers
-- placeholders by column, and the asymmetric
-- `(A,B,state=1) OR (B,A,state=2)` predicate loses its meaning when it does.
-- name: AcceptFriendInvite :exec
UPDATE user_edge SET state = 0, update_time = now()
WHERE ((source_id = $1 AND destination_id = $2) OR (source_id = $2 AND destination_id = $1))
  AND state IN (1, 2);

-- The invite pair above shares one position; the count moves only when those
-- rows were just created, so an existing pair can never double-count.
-- name: IncrementPairEdgeCount :exec
UPDATE users SET edge_count = edge_count + 1, update_time = now()
WHERE (id = $1::UUID OR id = $2::UUID)
  AND EXISTS (
    SELECT state FROM user_edge
    WHERE (source_id = $1::UUID AND destination_id = $2::UUID AND position = $3)
       OR (source_id = $2::UUID AND destination_id = $1::UUID AND position = $3)
  );

-- One directed edge, for the asymmetric delete the DeleteFriends runs:
-- my edge always goes, theirs only when it is not a block of me.
-- name: DeleteUserEdge :exec
DELETE FROM user_edge WHERE source_id = $1 AND destination_id = $2;
