-- name: CreateUser :one
INSERT INTO users (username, password_hash, role)
VALUES ($1, $2, $3)
RETURNING *;

-- name: GetUser :one
SELECT * FROM users
WHERE id = $1 LIMIT 1;

-- name: GetUserByUsername :one
SELECT * FROM users
WHERE username = $1 LIMIT 1;

-- name: CreateCircle :one
INSERT INTO circles (name, creator_id, created_at)
VALUES ($1, $2, $3)
RETURNING *;

-- name: GetCircle :one
SELECT id, name, creator_id, created_at FROM circles
WHERE id = $1 LIMIT 1;

-- name: AddCircleMember :exec
INSERT INTO circle_members (circle_id, user_id, clout)
VALUES ($1, $2, $3);

-- name: GetCircleMember :one
SELECT * FROM circle_members
WHERE circle_id = $1 AND user_id = $2 LIMIT 1;

-- name: UpdateCircleMemberClout :exec
UPDATE circle_members
SET clout = $3
WHERE circle_id = $1 AND user_id = $2;

-- name: ListCircleMembers :many
SELECT cm.*, u.username
FROM circle_members cm
JOIN users u ON u.id = cm.user_id
WHERE cm.circle_id = $1;

-- name: ListUserCircles :many
SELECT c.id, c.name, c.creator_id, c.created_at
FROM circles c
JOIN circle_members cm ON cm.circle_id = c.id
WHERE cm.user_id = $1
ORDER BY c.created_at DESC;

-- name: DeleteCircle :exec
DELETE FROM circles
WHERE id = $1;

-- name: CreateContest :one
INSERT INTO contests (circle_id, creator_id, question, status, min_stake, consumption_rate, created_at, closes_at, duration)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
RETURNING *;

-- name: GetContest :one
SELECT * FROM contests
WHERE id = $1 LIMIT 1;

-- name: UpdateContestStatus :exec
UPDATE contests
SET status = $2, result_option_id = $3
WHERE id = $1;

-- name: CreateOption :exec
INSERT INTO options (contest_id, option_id, text)
VALUES ($1, $2, $3);

-- name: ListContestOptions :many
SELECT * FROM options
WHERE contest_id = $1;

-- name: CreatePrediction :one
INSERT INTO predictions (contest_id, user_id, option_id, clout, created_at)
VALUES ($1, $2, $3, $4, $5)
RETURNING *;

-- name: ListContestPredictions :many
SELECT * FROM predictions
WHERE contest_id = $1;

-- name: ListContestsByCircle :many
SELECT c.*
FROM contests c
WHERE c.circle_id = $1
ORDER BY created_at DESC;

-- name: FindExpiredContests :many
SELECT * FROM contests
WHERE status IN ('OPEN', 'LOCKED')
  AND closes_at <= NOW();

-- name: UpdateContestStatusOnly :exec
UPDATE contests
SET status = $2
WHERE id = $1;
