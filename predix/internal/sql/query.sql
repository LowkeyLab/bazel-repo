-- name: CreateUser :one
INSERT INTO users (id, name, email)
VALUES ($1, $2, $3)
RETURNING *;

-- name: GetUser :one
SELECT * FROM users
WHERE id = $1 LIMIT 1;

-- name: GetUserByEmail :one
SELECT * FROM users
WHERE email = $1 LIMIT 1;

-- name: CreateCircle :one
INSERT INTO circles (id, name, invite_code, created_at)
VALUES ($1, $2, $3, $4)
RETURNING *;

-- name: GetCircle :one
SELECT * FROM circles
WHERE id = $1 LIMIT 1;

-- name: GetCircleByInviteCode :one
SELECT * FROM circles
WHERE invite_code = $1 LIMIT 1;

-- name: AddCircleMember :exec
INSERT INTO circle_members (circle_id, user_id, clout)
VALUES ($1, $2, $3);

-- name: GetCircleMember :one
SELECT * FROM circle_members
WHERE circle_id = $1 AND user_id = $2 LIMIT 1;

-- name: ListCircleMembers :many
SELECT * FROM circle_members
WHERE circle_id = $1;

-- name: CreateContest :one
INSERT INTO contests (id, circle_id, creator_id, question, status, created_at, expires_at)
VALUES ($1, $2, $3, $4, $5, $6, $7)
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
SELECT * FROM contests
WHERE circle_id = $1
ORDER BY created_at DESC;
