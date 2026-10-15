-- Agent 3: Groups. BaaS implementation
-- Roles in group_edge.state: 0=Superadmin, 1=Admin, 2=Member, 3=JoinRequest

-- name: CreateGroup :one
INSERT INTO groups (id, creator_id, name, description, avatar_url, lang_tag,
  metadata, edge_count, max_count, state)
VALUES ($1, $2, $3, $4, $5, $6, $7, 1, $8, $9)
RETURNING *;

-- A disabled (deleted) group reads as gone, exactly like the groupGet.
-- name: GetGroupById :one
SELECT * FROM groups WHERE id = $1 AND disable_time = '1970-01-01 00:00:00+00';

-- name: GetGroupByName :one
SELECT * FROM groups WHERE name = $1;

-- Permission is the handler's job (an admin as well as the creator may edit),
-- so this keys on the group alone.
-- name: UpdateGroup :exec
UPDATE groups SET name = $2, description = $3, avatar_url = $4, lang_tag = $5,
  metadata = $6, max_count = $7, update_time = now()
WHERE id = $1;

-- name: DeleteGroup :exec
UPDATE groups SET disable_time = now(), update_time = now() WHERE id = $1;

-- name: JoinGroupEdge :exec
INSERT INTO group_edge (source_id, destination_id, state, position)
VALUES ($1, $2, $3, $4)
ON CONFLICT (source_id, destination_id) DO UPDATE SET state = $3, update_time = now();

-- name: LeaveGroupEdge :exec
DELETE FROM group_edge WHERE source_id = $1 AND destination_id = $2;

-- name: UpdateGroupEdgeState :exec
UPDATE group_edge SET state = $3, update_time = now()
WHERE source_id = $1 AND destination_id = $2;

-- name: DeleteGroupEdge :exec
DELETE FROM group_edge WHERE source_id = $1 AND destination_id = $2;

-- name: GetGroupEdgeState :one
SELECT state FROM group_edge WHERE source_id = $1 AND destination_id = $2;

-- BaaS branches this search per filter combination and gives each branch its
-- own keyset cursor. Every branch that is not a name search orders live groups
-- oldest-update-first, so one query with "no filter" sentinels covers them all,
-- paged by offset rather than keyset.
-- name: ListGroups :many
SELECT * FROM groups
WHERE disable_time = '1970-01-01 00:00:00+00'
  AND ($1 = '' OR lang_tag = $1)
  AND ($2 < 0 OR state = $2)
  AND ($3 < 0 OR edge_count <= $3)
ORDER BY update_time ASC, edge_count ASC, id ASC LIMIT $4 OFFSET $5;

-- A name search is verbatim ILIKE against the pattern the client sent, and it
-- deliberately ignores the disable time — the branch does the same.
-- name: ListGroupsByName :many
SELECT * FROM groups
WHERE name ILIKE $1
ORDER BY name ASC LIMIT $2 OFFSET $3;

-- name: ListGroupsBySize :many
SELECT * FROM groups
WHERE disable_time = '1970-01-01 00:00:00+00'
  AND edge_count BETWEEN $1 AND $2
ORDER BY edge_count DESC LIMIT $3 OFFSET $4;

-- name: ListGroupsForUser :many
SELECT g.*, ge.state AS member_state FROM group_edge ge
JOIN groups g ON g.id = ge.destination_id
WHERE ge.source_id = $1 AND ge.state < 3 AND g.disable_time = '1970-01-01 00:00:00+00'
ORDER BY ge.state ASC, g.name ASC LIMIT $2;

-- name: ListGroupUsers :many
SELECT CAST(u.id AS UUID) AS id, u.username, u.display_name, u.avatar_url, u.metadata, u.create_time, u.update_time,
       ge.state, ge.update_time AS edge_update_time
FROM group_edge ge
JOIN users u ON u.id = ge.source_id
WHERE ge.destination_id = $1
ORDER BY ge.state ASC, u.username ASC LIMIT $2;

-- name: IncrementGroupEdgeCount :exec
UPDATE groups SET edge_count = edge_count + $2, update_time = now() WHERE id = $1;

-- The capacity gate lives in the UPDATE, not in the handler: a group that is
-- already at max_count matches no row, so the generated call returns
-- RowNotFound and the handler answers "Group is full." (core_group.go:419).
-- name: ExpandGroupEdgeCount :one
UPDATE groups SET edge_count = edge_count + 1, update_time = now()
WHERE id = $1 AND edge_count + 1 <= max_count
RETURNING edge_count;

-- Members leaving: floor at 1 so the CHECK (edge_count >= 1) can never trip,
-- because a group always keeps its creator's edge.
-- name: ShrinkGroupEdgeCount :exec
UPDATE groups SET edge_count = edge_count - 1, update_time = now()
WHERE id = $1 AND edge_count > 1;

-- "Am I the last superadmin?" is a count, not a guess (core_group.go:1057).
-- name: CountGroupRoleHolders :one
SELECT COUNT(state) AS holders FROM group_edge WHERE destination_id = $1 AND state = $2;

-- Promotion walks one step up the role ladder and cannot cross the caller's
-- own rank or reach superadmin (core_group.go: state = state - 1 WHERE state > 0
-- AND state > caller_state AND state <= 2).
-- name: PromoteGroupEdge :exec
UPDATE group_edge SET state = state - 1, update_time = now()
WHERE destination_id = $1 AND source_id = $2 AND state > 0 AND state > $3 AND state <= 2;

-- Demotion walks one step down and never demotes a superadmin unless the
-- caller is one (core_group.go: state = state + 1 WHERE state >= caller_state
-- AND state < 2).
-- name: DemoteGroupEdge :exec
UPDATE group_edge SET state = state + 1, update_time = now()
WHERE destination_id = $1 AND source_id = $2 AND state >= $3 AND state < 2;

-- Banning replaces the edge with state 4; the unique (source, destination) pair
-- means a re-join attempt hits this row, never a fresh one (core_group.go:49).
-- name: BanGroupEdge :exec
INSERT INTO group_edge (source_id, destination_id, state, position)
VALUES ($1, $2, 4, $3)
ON CONFLICT (source_id, destination_id) DO UPDATE SET state = 4, update_time = now();

-- Member lists page by (state, position) and, with no state filter, hide the
-- banned edges (core_group.go limits state to 0..3 when unfiltered).
-- name: ListGroupUsersPaged :many
SELECT CAST(u.id AS UUID) AS id, u.username, u.display_name, u.avatar_url, u.metadata, u.create_time, u.update_time,
       ge.state, ge.position AS edge_position, ge.update_time AS edge_update_time
FROM group_edge ge
JOIN users u ON u.id = ge.source_id
WHERE ge.destination_id = $1 AND ge.state < 3
  AND (ge.state > $2 OR (ge.state = $2 AND ge.position > $3))
ORDER BY ge.state ASC, ge.position ASC LIMIT $4;

-- name: ListGroupUsersByStatePaged :many
SELECT CAST(u.id AS UUID) AS id, u.username, u.display_name, u.avatar_url, u.metadata, u.create_time, u.update_time,
       ge.state, ge.position AS edge_position, ge.update_time AS edge_update_time
FROM group_edge ge
JOIN users u ON u.id = ge.source_id
WHERE ge.destination_id = $1 AND ge.state = $2 AND ge.position > $3
ORDER BY ge.position ASC LIMIT $4;

-- A join request in a closed group is announced to everyone who can approve it
-- (core_group.go notifies the superadmins and admins of the group).
-- name: ListGroupAdmins :many
SELECT source_id FROM group_edge WHERE destination_id = $1 AND state <= $2;

-- The groups a user belongs to, paged by (state, position). `state = -1` is the
-- "no state filter" sentinel, since every real role is >= 0.
-- name: ListGroupsForUserPaged :many
SELECT g.*, ge.state AS member_state, ge.position AS member_position FROM group_edge ge
JOIN groups g ON g.id = ge.destination_id
WHERE ge.source_id = $1
  AND (ge.state > $2 OR (ge.state = $2 AND ge.position > $3))
  AND g.disable_time = '1970-01-01 00:00:00+00'
ORDER BY ge.state ASC, ge.position ASC LIMIT $4;

-- name: ListGroupsForUserByStatePaged :many
SELECT g.*, ge.state AS member_state, ge.position AS member_position FROM group_edge ge
JOIN groups g ON g.id = ge.destination_id
WHERE ge.source_id = $1 AND ge.state = $2 AND ge.position > $3
  AND g.disable_time = '1970-01-01 00:00:00+00'
ORDER BY ge.position ASC LIMIT $4;
