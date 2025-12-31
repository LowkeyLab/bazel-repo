package rest

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

type contestResponseBody struct {
	ID        int32   `json:"id"`
	CircleIDs []int32 `json:"circle_ids"`
	CreatorID int32   `json:"creator_id"`
	Question  string  `json:"question"`
	Options   []struct {
		ID   int    `json:"id"`
		Text string `json:"text"`
	} `json:"options"`
	Predictions []struct {
		UserID    int32     `json:"user_id"`
		OptionID  int       `json:"option_id"`
		Clout     int       `json:"clout"`
		Timestamp time.Time `json:"timestamp"`
	} `json:"predictions"`
	Status         string    `json:"status"`
	ResultOptionID *int      `json:"result_option_id,omitempty"`
	CreatedAt      time.Time `json:"created_at"`
	ExpiresAt      time.Time `json:"expires_at"`
}

func setupTestRouter(t *testing.T) (*gin.Engine, *pgxpool.Pool, *service.Service, *db.Queries) {
	t.Helper()
	gin.SetMode(gin.TestMode)

	pool := testutil.SetupTestDB(t)
	repo := repository.NewPostgres(pool)
	svc := service.NewService(repo)
	handler := NewHandler(svc)

	r := gin.New()
	handler.RegisterRoutes(r)

	return r, pool, svc, db.New(pool)
}

func createTestUser(t *testing.T, pool *pgxpool.Pool, name, email string) user.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)

	result, err := q.CreateUser(ctx, db.CreateUserParams{
		Name:  name,
		Email: email,
	})
	require.NoError(t, err)

	return user.ID(result.ID)
}

func createTestCircle(t *testing.T, pool *pgxpool.Pool, name string, creatorID user.ID) circle.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)

	result, err := q.CreateCircle(ctx, db.CreateCircleParams{
		Name:      name,
		CreatedAt: pgtype.Timestamp{Time: time.Now(), Valid: true},
	})
	require.NoError(t, err)

	err = q.AddCircleMember(ctx, db.AddCircleMemberParams{
		CircleID: result.ID,
		UserID:   int32(creatorID),
		Clout:    1000,
	})
	require.NoError(t, err)

	return circle.ID(result.ID)
}

func TestCreateContestHandler(t *testing.T) {
	router, pool, _, queries := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "Alice", "alice@example.com")
	circleID := createTestCircle(t, pool, "Study Group", creatorID)

	expiresAt := time.Now().Add(24 * time.Hour)

	body := fmt.Sprintf(`{
		"circle_ids": [%d],
		"creator_id": %d,
		"question": "Who will win the Super Bowl?",
		"options": ["Team A", "Team B"],
		"expires_at": "%s"
	}`, circleID, creatorID, expiresAt.Format(time.RFC3339))

	req := httptest.NewRequest(http.MethodPost, "/contests", bytes.NewBufferString(body))
	req.Header.Set("Content-Type", "application/json")

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusCreated, resp.Code)

	var payload contestResponseBody
	require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &payload))
	assert.Equal(t, "Who will win the Super Bowl?", payload.Question)
	assert.Len(t, payload.Options, 2)
	assert.Equal(t, "OPEN", payload.Status)
	assert.Equal(t, int32(creatorID), payload.CreatorID)
	assert.Contains(t, payload.CircleIDs, int32(circleID))

	ctx := context.Background()
	dbContest, err := queries.GetContest(ctx, payload.ID)
	require.NoError(t, err)
	assert.Equal(t, "Who will win the Super Bowl?", dbContest.Question)
}

func TestMakePredictionHandler(t *testing.T) {
	router, pool, svc, queries := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "Bob", "bob@example.com")
	predictorID := createTestUser(t, pool, "Charlie", "charlie@example.com")
	circleID := createTestCircle(t, pool, "Sports Fans", creatorID)

	ctx := context.Background()
	expiresAt := time.Now().Add(24 * time.Hour)
	createdContest, err := svc.CreateContest(ctx, []circle.ID{circleID}, creatorID, "Who wins?", []string{"Option A", "Option B"}, expiresAt)
	require.NoError(t, err)

	body := fmt.Sprintf(`{
		"user_id": %d,
		"option_id": 1,
		"clout": 100
	}`, predictorID)

	req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/contests/%d/predictions", createdContest.ID), bytes.NewBufferString(body))
	req.Header.Set("Content-Type", "application/json")

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusCreated, resp.Code)

	dbPredictions, err := queries.ListContestPredictions(ctx, int32(createdContest.ID))
	require.NoError(t, err)
	assert.Len(t, dbPredictions, 1)
	assert.Equal(t, int32(predictorID), dbPredictions[0].UserID)
	assert.Equal(t, int32(1), dbPredictions[0].OptionID)
	assert.Equal(t, int32(100), dbPredictions[0].Clout)
}

func TestResolveContestHandler(t *testing.T) {
	router, pool, svc, queries := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "David", "david@example.com")
	circleID := createTestCircle(t, pool, "Predictors", creatorID)

	ctx := context.Background()
	expiresAt := time.Now().Add(24 * time.Hour)
	createdContest, err := svc.CreateContest(ctx, []circle.ID{circleID}, creatorID, "Test Question?", []string{"Yes", "No"}, expiresAt)
	require.NoError(t, err)

	body := `{"winning_option_id": 1}`
	req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/contests/%d/resolve", createdContest.ID), bytes.NewBufferString(body))
	req.Header.Set("Content-Type", "application/json")

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusOK, resp.Code)

	dbContest, err := queries.GetContest(ctx, int32(createdContest.ID))
	require.NoError(t, err)
	assert.Equal(t, "RESOLVED", dbContest.Status)
	require.True(t, dbContest.ResultOptionID.Valid)
	assert.Equal(t, int32(1), dbContest.ResultOptionID.Int32)
}

func TestGetContest(t *testing.T) {
	router, pool, svc, _ := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "Eve", "eve@example.com")
	circleID := createTestCircle(t, pool, "Test Circle", creatorID)

	ctx := context.Background()
	expiresAt := time.Now().Add(24 * time.Hour)
	createdContest, err := svc.CreateContest(ctx, []circle.ID{circleID}, creatorID, "Will it rain?", []string{"Yes", "No", "Maybe"}, expiresAt)
	require.NoError(t, err)

	req := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/contests/%d", createdContest.ID), nil)
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusOK, resp.Code)

	var payload contestResponseBody
	require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &payload))
	assert.Equal(t, int32(createdContest.ID), payload.ID)
	assert.Equal(t, "Will it rain?", payload.Question)
	assert.Len(t, payload.Options, 3)
	assert.Equal(t, "OPEN", payload.Status)
}

func TestGetContest_NotFound(t *testing.T) {
	router, _, _, _ := setupTestRouter(t)

	req := httptest.NewRequest(http.MethodGet, "/contests/99999", nil)
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	assert.Equal(t, http.StatusNotFound, resp.Code)
}

func TestCreateContest_InvalidRequest(t *testing.T) {
	router, _, _, _ := setupTestRouter(t)

	body := `{"invalid": "data"}`
	req := httptest.NewRequest(http.MethodPost, "/contests", bytes.NewBufferString(body))
	req.Header.Set("Content-Type", "application/json")

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	assert.Equal(t, http.StatusBadRequest, resp.Code)
}

func TestMakePrediction_ContestNotFound(t *testing.T) {
	router, _, _, _ := setupTestRouter(t)

	body := `{"user_id": 1, "option_id": 1, "clout": 100}`
	req := httptest.NewRequest(http.MethodPost, "/contests/99999/predictions", bytes.NewBufferString(body))
	req.Header.Set("Content-Type", "application/json")

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	assert.Equal(t, http.StatusNotFound, resp.Code)
}
