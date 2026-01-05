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
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	circlerepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	circleservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

type contestResponseBody struct {
	ID        int32  `json:"id"`
	CircleID  int32  `json:"circle_id"`
	CreatorID int32  `json:"creator_id"`
	Question  string `json:"question"`
	MinStake  int    `json:"min_stake"`
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
	ClosesAt       time.Time `json:"closes_at"`
	Duration       string    `json:"duration"`
}

func setupTestRouter(t *testing.T, pool *pgxpool.Pool) (*gin.Engine, *service.Service, *db.Queries) {
	t.Helper()
	gin.SetMode(gin.TestMode)
	repo := repository.NewPostgres(pool)
	circleRepo := circlerepo.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	contestSvc := service.NewService(repo, clock.RealClock{})
	circleSvc := circleservice.NewService(circleRepo, userRepo, contestSvc)
	handler := NewHandler(contestSvc, circleSvc)

	r := gin.New()
	authGroup := r.Group("/protected")
	authGroup.Use(auth.TestMiddleware())
	handler.RegisterRoutes(authGroup)

	return r, contestSvc, db.New(pool)
}

func createTestUser(t *testing.T, pool *pgxpool.Pool, username string) user.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)
	passwordHash := "hash"

	result, err := q.CreateUser(ctx, db.CreateUserParams{
		Username:     username,
		PasswordHash: passwordHash,
		Role:         db.UserRoleMember,
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
		CreatorID: int32(creatorID),
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

func TestContestHandlers(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, pool *pgxpool.Pool) {
		build := func(t *testing.T) (*gin.Engine, *service.Service, *db.Queries) {
			testutil.ResetTables(t, pool)
			return setupTestRouter(t, pool)
		}

		t.Run("CreateContestHandler", func(t *testing.T) {
			router, _, queries := build(t)

			creatorID := createTestUser(t, pool, "alice")
			circleID := createTestCircle(t, pool, "Study Group", creatorID)

			body := fmt.Sprintf(`{
				"circle_id": %d,
				"question": "Who will win the Super Bowl?",
				"options": ["Team A", "Team B"],
				"min_stake": 100,
				"expiration_duration": "1d"
			}`, circleID)

			req := httptest.NewRequest(http.MethodPost, "/protected/contests", bytes.NewBufferString(body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusCreated, resp.Code)

			var payload contestResponseBody
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &payload))
			assert.Equal(t, "Who will win the Super Bowl?", payload.Question)
			assert.Len(t, payload.Options, 2)
			assert.Equal(t, "OPEN", payload.Status)
			assert.Equal(t, int32(creatorID), payload.CreatorID)
			assert.Equal(t, 100, payload.MinStake)
			assert.Equal(t, int32(circleID), payload.CircleID)

			ctx := context.Background()
			dbContest, err := queries.GetContest(ctx, payload.ID)
			require.NoError(t, err)
			assert.Equal(t, "Who will win the Super Bowl?", dbContest.Question)
		})

		t.Run("CreateContestHandler_InvalidMinStake", func(t *testing.T) {
			router, _, _ := build(t)

			creatorID := createTestUser(t, pool, "alice-invalid")
			circleID := createTestCircle(t, pool, "Validation Circle", creatorID)

			body := fmt.Sprintf(`{
				"circle_id": %d,
				"question": "Invalid stake?",
				"options": ["Team A", "Team B"],
				"min_stake": 5,
				"expiration_duration": "1d"
			}`, circleID)

			req := httptest.NewRequest(http.MethodPost, "/protected/contests", bytes.NewBufferString(body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusBadRequest, resp.Code)
			assert.Contains(t, resp.Body.String(), "min stake must be one of")
		})

		t.Run("MakePredictionHandler", func(t *testing.T) {
			router, svc, queries := build(t)

			creatorID := createTestUser(t, pool, "bob")
			predictorID := createTestUser(t, pool, "charlie")
			circleID := createTestCircle(t, pool, "Sports Fans", creatorID)

			// Add predictor to the circle
			q := db.New(pool)
			ctx := context.Background()
			err := q.AddCircleMember(ctx, db.AddCircleMemberParams{
				CircleID: int32(circleID),
				UserID:   int32(predictorID),
				Clout:    1000,
			})
			require.NoError(t, err)

			createdContest, err := svc.CreateContest(ctx, circleID, creatorID, "Who wins?", []string{"Option A", "Option B"}, contest.Duration1Day, 0)
			require.NoError(t, err)

			body := fmt.Sprintf(`{
				"circle_id": %d,
				"option_id": 1,
				"clout": 100
			}`, circleID)

			req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/contests/%d/predictions", createdContest.ID), bytes.NewBufferString(body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", predictorID))

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusCreated, resp.Code)

			dbPredictions, err := queries.ListContestPredictions(ctx, int32(createdContest.ID))
			require.NoError(t, err)
			assert.Len(t, dbPredictions, 1)
			assert.Equal(t, int32(predictorID), dbPredictions[0].UserID)
			assert.Equal(t, int32(1), dbPredictions[0].OptionID)
			assert.Equal(t, int32(100), dbPredictions[0].Clout)
		})

		t.Run("MakePredictionHandler_UpdatesExistingStake", func(t *testing.T) {
			router, svc, queries := build(t)

			creatorID := createTestUser(t, pool, "predict-update-creator")
			predictorID := createTestUser(t, pool, "predict-updater")
			circleID := createTestCircle(t, pool, "Update Circle", creatorID)

			// Add predictor to the circle
			q := db.New(pool)
			ctx := context.Background()
			err := q.AddCircleMember(ctx, db.AddCircleMemberParams{
				CircleID: int32(circleID),
				UserID:   int32(predictorID),
				Clout:    1000,
			})
			require.NoError(t, err)

			createdContest, err := svc.CreateContest(ctx, circleID, creatorID, "Who wins?", []string{"Option A", "Option B"}, contest.Duration1Day, 100)
			require.NoError(t, err)

			firstBody := fmt.Sprintf(`{"circle_id": %d, "option_id": 1, "clout": 200}`, circleID)
			firstReq := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/contests/%d/predictions", createdContest.ID), bytes.NewBufferString(firstBody))
			firstReq.Header.Set("Content-Type", "application/json")
			firstReq.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", predictorID))
			firstResp := httptest.NewRecorder()
			router.ServeHTTP(firstResp, firstReq)

			require.Equal(t, http.StatusCreated, firstResp.Code)

			secondBody := fmt.Sprintf(`{"circle_id": %d, "option_id": 1, "clout": 120}`, circleID)
			secondReq := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/contests/%d/predictions", createdContest.ID), bytes.NewBufferString(secondBody))
			secondReq.Header.Set("Content-Type", "application/json")
			secondReq.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", predictorID))
			secondResp := httptest.NewRecorder()
			router.ServeHTTP(secondResp, secondReq)

			require.Equal(t, http.StatusCreated, secondResp.Code)

			dbPredictions, err := queries.ListContestPredictions(ctx, int32(createdContest.ID))
			require.NoError(t, err)
			require.Len(t, dbPredictions, 1)
			assert.Equal(t, int32(120), dbPredictions[0].Clout)

			memberRecord, err := queries.GetCircleMember(ctx, db.GetCircleMemberParams{
				CircleID: int32(circleID),
				UserID:   int32(predictorID),
			})
			require.NoError(t, err)
			assert.Equal(t, int32(880), memberRecord.Clout)
		})

		t.Run("ResolveContestHandler", func(t *testing.T) {
			router, svc, queries := build(t)

			creatorID := createTestUser(t, pool, "david")
			circleID := createTestCircle(t, pool, "Predictors", creatorID)

			ctx := context.Background()
			createdContest, err := svc.CreateContest(ctx, circleID, creatorID, "Test Question?", []string{"Yes", "No"}, contest.Duration1Day, 0)
			require.NoError(t, err)

			body := `{"winning_option_id": 1}`
			req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/contests/%d/resolve", createdContest.ID), bytes.NewBufferString(body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusOK, resp.Code)

			dbContest, err := queries.GetContest(ctx, int32(createdContest.ID))
			require.NoError(t, err)
			assert.Equal(t, "RESOLVED", dbContest.Status)
			require.True(t, dbContest.ResultOptionID.Valid)
			assert.Equal(t, int32(1), dbContest.ResultOptionID.Int32)
		})

		t.Run("ResolveContestHandler_OnlyCreatorCanResolve", func(t *testing.T) {
			router, svc, queries := build(t)

			creatorID := createTestUser(t, pool, "isabel")
			nonCreatorID := createTestUser(t, pool, "jack")
			circleID := createTestCircle(t, pool, "Guarded Circle", creatorID)

			ctx := context.Background()
			createdContest, err := svc.CreateContest(ctx, circleID, creatorID, "Resolve?", []string{"Yes", "No"}, contest.Duration1Day, 0)
			require.NoError(t, err)

			body := `{"winning_option_id": 1}`
			req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/contests/%d/resolve", createdContest.ID), bytes.NewBufferString(body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", nonCreatorID))

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusForbidden, resp.Code)

			dbContest, err := queries.GetContest(ctx, int32(createdContest.ID))
			require.NoError(t, err)
			assert.Equal(t, "OPEN", dbContest.Status)
			assert.False(t, dbContest.ResultOptionID.Valid)
		})

		t.Run("GetContest", func(t *testing.T) {
			router, svc, _ := build(t)

			creatorID := createTestUser(t, pool, "eve")
			circleID := createTestCircle(t, pool, "Test Circle", creatorID)

			ctx := context.Background()
			createdContest, err := svc.CreateContest(ctx, circleID, creatorID, "Will it rain?", []string{"Yes", "No", "Maybe"}, contest.Duration1Day, 0)
			require.NoError(t, err)

			req := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/contests/%d", createdContest.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusOK, resp.Code)

			var payload contestResponseBody
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &payload))
			assert.Equal(t, int32(createdContest.ID), payload.ID)
			assert.Equal(t, "Will it rain?", payload.Question)
			assert.Len(t, payload.Options, 3)
			assert.Equal(t, "OPEN", payload.Status)
			assert.Equal(t, 10, payload.MinStake)
		})

		t.Run("GetContest_NotFound", func(t *testing.T) {
			router, _, _ := build(t)

			userID := createTestUser(t, pool, "frank")

			req := httptest.NewRequest(http.MethodGet, "/protected/contests/99999", nil)
			req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", userID))
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusNotFound, resp.Code)
		})

		t.Run("CreateContest_InvalidRequest", func(t *testing.T) {
			router, _, _ := build(t)

			body := `{"invalid": "data"}`
			req := httptest.NewRequest(http.MethodPost, "/protected/contests", bytes.NewBufferString(body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set(auth.TestUserIDHeader, "1")

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusBadRequest, resp.Code)
		})

		t.Run("MakePrediction_ContestNotFound", func(t *testing.T) {
			router, _, _ := build(t)

			body := `{"option_id": 1, "clout": 100}`
			req := httptest.NewRequest(http.MethodPost, "/protected/contests/99999/predictions", bytes.NewBufferString(body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set(auth.TestUserIDHeader, "1")

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusNotFound, resp.Code)
		})

		t.Run("GetPayoutBreakdown_Success", func(t *testing.T) {
			router, svc, _ := build(t)

			ctx := context.Background()
			creatorID := createTestUser(t, pool, "carol")
			user1ID := createTestUser(t, pool, "dave")
			user2ID := createTestUser(t, pool, "eve")
			circleID := createTestCircle(t, pool, "Payout Circle", creatorID)

			// Create contest with 2 options
			opts := []string{"Option A", "Option B"}
			createdContest, err := svc.CreateContest(ctx, circleID, creatorID, "Payout test?", opts, contest.Duration1Day, 0)
			require.NoError(t, err)

			// Add predictions
			err = svc.RecordPrediction(ctx, createdContest.ID, user1ID, 1, 100)
			require.NoError(t, err)
			err = svc.RecordPrediction(ctx, createdContest.ID, user2ID, 1, 200)
			require.NoError(t, err)

			// Resolve contest
			_, err = svc.ResolveContestAndCalculatePayouts(ctx, createdContest.ID, creatorID, 1)
			require.NoError(t, err)

			// Request payout breakdown
			req := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/contests/%d/payout-breakdown", createdContest.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusOK, resp.Code)

			var payload payoutBreakdownResponse
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &payload))

			// Verify calculations
			assert.Equal(t, 300, payload.TotalPot)
			assert.Equal(t, 30, payload.CloutConsumed)     // 10% of 300
			assert.Equal(t, 270, payload.DistributablePot) // 90% of 300
			assert.Len(t, payload.Winners, 2)

			// Verify winner payouts
			// User1: 100 stake + (100/300)*270 share = 100 + 90 = 190
			// User2: 200 stake + (200/300)*270 share = 200 + 180 = 380
			assert.Equal(t, int32(user1ID), payload.Winners[0].UserID)
			assert.Equal(t, 100, payload.Winners[0].Stake)
			assert.Equal(t, 90, payload.Winners[0].Share)
			assert.Equal(t, 190, payload.Winners[0].Total)

			assert.Equal(t, int32(user2ID), payload.Winners[1].UserID)
			assert.Equal(t, 200, payload.Winners[1].Stake)
			assert.Equal(t, 180, payload.Winners[1].Share)
			assert.Equal(t, 380, payload.Winners[1].Total)

			assert.Equal(t, 570, payload.TotalDistributed)
		})

		t.Run("GetPayoutBreakdown_NotResolved", func(t *testing.T) {
			router, svc, _ := build(t)

			ctx := context.Background()
			creatorID := createTestUser(t, pool, "grace")
			circleID := createTestCircle(t, pool, "Unresolved Circle", creatorID)

			opts := []string{"Option A", "Option B"}
			createdContest, err := svc.CreateContest(ctx, circleID, creatorID, "Not resolved?", opts, contest.Duration1Day, 0)
			require.NoError(t, err)

			// Try to get payout breakdown for non-resolved contest
			req := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/contests/%d/payout-breakdown", createdContest.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusBadRequest, resp.Code)
		})

		t.Run("GetPayoutBreakdown_NotFound", func(t *testing.T) {
			router, _, _ := build(t)

			creatorID := createTestUser(t, pool, "henry")

			req := httptest.NewRequest(http.MethodGet, "/protected/contests/99999/payout-breakdown", nil)
			req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusNotFound, resp.Code)
		})
	})
}
