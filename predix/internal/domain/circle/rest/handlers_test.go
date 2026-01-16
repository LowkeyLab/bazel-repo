package rest

import (
	"bytes"
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"

	sloggin "github.com/gin-contrib/slog"
	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	circlerepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/sse"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	contestrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

type circleResponseBody struct {
	ID      int32  `json:"id"`
	Name    string `json:"name"`
	Members []struct {
		UserID string `json:"user_id"`
		Clout  int    `json:"clout"`
	} `json:"members"`
}

type TestTxManager struct {
	sqlDB *sql.DB
}

func (m *TestTxManager) RunInTx(ctx context.Context, fn func(ctx context.Context) error) error {
	return db.RunInTx(ctx, m.sqlDB, fn)
}

func setupTestRouter(t *testing.T, sqlDB *sql.DB) (*gin.Engine, *service.Service, *db.Queries) {
	t.Helper()
	gin.SetMode(gin.TestMode)
	repo := circlerepo.NewPostgres(sqlDB)
	userRepo := userrepo.NewPostgres(sqlDB)
	contestRepo := contestrepo.NewPostgres(sqlDB)
	clk := clock.RealClock{}
	txm := &TestTxManager{sqlDB: sqlDB}
	svc := service.NewService(repo, userRepo, contestRepo, clk, txm, sse.NewManager())
	handler := NewHandler(svc)

	r := gin.New()
	r.Use(sloggin.SetLogger())
	authGroup := r.Group("/protected")
	authGroup.Use(auth.TestMiddleware())
	handler.RegisterRoutes(authGroup)

	return r, svc, db.New(sqlDB)
}

func createTestUser(t *testing.T, sqlDB *sql.DB, username string) user.ID {
	return createUserWithRole(t, sqlDB, username, db.UserRoleMember)
}

func createUserWithRole(t *testing.T, sqlDB *sql.DB, username string, role db.UserRole) user.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(sqlDB)
	passwordHash := "hash"

	result, err := q.CreateUser(ctx, db.CreateUserParams{
		Username:     username,
		PasswordHash: passwordHash,
		Role:         role,
	})
	require.NoError(t, err)

	return user.ID(result.Uuid)
}

func TestCircleHandlers(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, sqlDB *sql.DB) {
		build := func(t *testing.T) (*gin.Engine, *service.Service, *db.Queries) {
			testutil.ResetTables(t, sqlDB)
			return setupTestRouter(t, sqlDB)
		}

		t.Run("CreateCircleHandler", func(t *testing.T) {
			router, _, queries := build(t)

			creatorID := createTestUser(t, sqlDB, "alice")

			body := `{"name":"Study Group"}`
			req := httptest.NewRequest(http.MethodPost, "/protected/circles", bytes.NewBufferString(body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusCreated, resp.Code)

			var payload circleResponseBody
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &payload))
			assert.Equal(t, "Study Group", payload.Name)
			assert.Len(t, payload.Members, 1)
			assert.Equal(t, uuid.UUID(creatorID).String(), payload.Members[0].UserID)

			ctx := context.Background()
			dbCircle, err := queries.GetCircle(ctx, payload.ID)
			require.NoError(t, err)
			assert.Equal(t, "Study Group", dbCircle.Name)

			dbMember, err := queries.GetCircleMember(ctx, db.GetCircleMemberParams{
				CircleID: payload.ID,
				UserID:   uuid.UUID(creatorID),
			})
			require.NoError(t, err)
			assert.Equal(t, int32(1000), dbMember.Clout)
		})

		t.Run("AddMemberHandler", func(t *testing.T) {
			router, svc, queries := build(t)

			creatorID := createTestUser(t, sqlDB, "bob")
			joinerID := createTestUser(t, sqlDB, "charlie")

			ctx := context.Background()
			createdCircle, err := svc.CreateCircle(ctx, "Readers", creatorID)
			require.NoError(t, err)

			body := fmt.Sprintf(`{"user_id":"%s"}`, uuid.UUID(joinerID))
			req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/circles/%d/members", createdCircle.ID), bytes.NewBufferString(body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusCreated, resp.Code)

			memberRecord, err := queries.GetCircleMember(ctx, db.GetCircleMemberParams{
				CircleID: int32(createdCircle.ID),
				UserID:   uuid.UUID(joinerID),
			})
			require.NoError(t, err)
			assert.Equal(t, int32(1000), memberRecord.Clout)

			getReq := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d", createdCircle.ID), nil)
			getReq.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())
			getResp := httptest.NewRecorder()
			router.ServeHTTP(getResp, getReq)

			require.Equal(t, http.StatusOK, getResp.Code)
			var circlePayload circleResponseBody
			require.NoError(t, json.Unmarshal(getResp.Body.Bytes(), &circlePayload))

			assert.Equal(t, createdCircle.Name, circlePayload.Name)
			assert.Len(t, circlePayload.Members, 2)
		})

		t.Run("DeleteCircle_ByCreator", func(t *testing.T) {
			router, svc, queries := build(t)

			creatorID := createTestUser(t, sqlDB, "creator")

			ctx := context.Background()
			createdCircle, err := svc.CreateCircle(ctx, "Writers", creatorID)
			require.NoError(t, err)

			req := httptest.NewRequest(http.MethodDelete, fmt.Sprintf("/protected/circles/%d", createdCircle.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusNoContent, resp.Code)
			_, err = queries.GetCircle(ctx, int32(createdCircle.ID))
			assert.ErrorIs(t, err, sql.ErrNoRows)
		})

		t.Run("DeleteCircle_ByAdmin", func(t *testing.T) {
			router, svc, queries := build(t)

			creatorID := createTestUser(t, sqlDB, "creator")
			adminID := createUserWithRole(t, sqlDB, "admin", db.UserRoleAdmin)

			ctx := context.Background()
			createdCircle, err := svc.CreateCircle(ctx, "Chess", creatorID)
			require.NoError(t, err)

			req := httptest.NewRequest(http.MethodDelete, fmt.Sprintf("/protected/circles/%d", createdCircle.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(adminID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusNoContent, resp.Code)
			_, err = queries.GetCircle(ctx, int32(createdCircle.ID))
			assert.ErrorIs(t, err, sql.ErrNoRows)
		})

		t.Run("DeleteCircle_Forbidden", func(t *testing.T) {
			router, svc, _ := build(t)

			creatorID := createTestUser(t, sqlDB, "creator")
			memberID := createTestUser(t, sqlDB, "member")

			ctx := context.Background()
			createdCircle, err := svc.CreateCircle(ctx, "Gamers", creatorID)
			require.NoError(t, err)

			req := httptest.NewRequest(http.MethodDelete, fmt.Sprintf("/protected/circles/%d", createdCircle.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(memberID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusForbidden, resp.Code)
		})

		t.Run("GetCircle_NotFound", func(t *testing.T) {
			router, _, _ := build(t)

			userID := createTestUser(t, sqlDB, "dana")

			req := httptest.NewRequest(http.MethodGet, "/protected/circles/99999", nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(userID).String())
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusNotFound, resp.Code)
		})

		t.Run("ListUserCircles", func(t *testing.T) {
			router, svc, _ := build(t)

			user1ID := createTestUser(t, sqlDB, "user1")
			user2ID := createTestUser(t, sqlDB, "user2")

			ctx := context.Background()
			circle1, err := svc.CreateCircle(ctx, "Book Club", user1ID)
			require.NoError(t, err)

			_, err = svc.CreateCircle(ctx, "Movie Night", user1ID)
			require.NoError(t, err)

			err = svc.AddMember(ctx, circle1.ID, user2ID)
			require.NoError(t, err)

			req := httptest.NewRequest(http.MethodGet, "/protected/circles", nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(user1ID).String())
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusOK, resp.Code)
			var circles []circleResponseBody
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &circles))

			assert.Len(t, circles, 2)
			circleNames := map[string]bool{}
			for _, c := range circles {
				circleNames[c.Name] = true
			}
			assert.True(t, circleNames["Book Club"])
			assert.True(t, circleNames["Movie Night"])

			req2 := httptest.NewRequest(http.MethodGet, "/protected/circles", nil)
			req2.Header.Set(auth.TestUserIDHeader, uuid.UUID(user2ID).String())
			resp2 := httptest.NewRecorder()

			router.ServeHTTP(resp2, req2)

			require.Equal(t, http.StatusOK, resp2.Code)
			var circles2 []circleResponseBody
			require.NoError(t, json.Unmarshal(resp2.Body.Bytes(), &circles2))

			assert.Len(t, circles2, 1)
			assert.Equal(t, "Book Club", circles2[0].Name)
		})

		t.Run("ListUserCircles_Empty", func(t *testing.T) {
			router, _, _ := build(t)

			userID := createTestUser(t, sqlDB, "lonely")

			req := httptest.NewRequest(http.MethodGet, "/protected/circles", nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(userID).String())
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusOK, resp.Code)
			var circles []circleResponseBody
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &circles))

			assert.Len(t, circles, 0)
		})

		t.Run("ListUserCircles_Unauthorized", func(t *testing.T) {
			router, _, _ := build(t)

			req := httptest.NewRequest(http.MethodGet, "/protected/circles", nil)
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusUnauthorized, resp.Code)
		})

		t.Run("JoinCircle_Success", func(t *testing.T) {
			router, svc, queries := build(t)

			creatorID := createTestUser(t, sqlDB, "creator")
			joinerID := createTestUser(t, sqlDB, "joiner")

			ctx := context.Background()
			createdCircle, err := svc.CreateCircle(ctx, "Open Circle", creatorID)
			require.NoError(t, err)

			req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/circles/%d/join", createdCircle.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(joinerID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusCreated, resp.Code)

			memberRecord, err := queries.GetCircleMember(ctx, db.GetCircleMemberParams{
				CircleID: int32(createdCircle.ID),
				UserID:   uuid.UUID(joinerID),
			})
			require.NoError(t, err)
			assert.Equal(t, int32(1000), memberRecord.Clout)

			getReq := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d", createdCircle.ID), nil)
			getReq.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())
			getResp := httptest.NewRecorder()
			router.ServeHTTP(getResp, getReq)

			require.Equal(t, http.StatusOK, getResp.Code)
			var circlePayload circleResponseBody
			require.NoError(t, json.Unmarshal(getResp.Body.Bytes(), &circlePayload))

			assert.Equal(t, createdCircle.Name, circlePayload.Name)
			assert.Len(t, circlePayload.Members, 2)
		})

		t.Run("JoinCircle_AlreadyMember", func(t *testing.T) {
			router, svc, _ := build(t)

			creatorID := createTestUser(t, sqlDB, "creator")

			ctx := context.Background()
			createdCircle, err := svc.CreateCircle(ctx, "Exclusive Circle", creatorID)
			require.NoError(t, err)

			req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/circles/%d/join", createdCircle.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusBadRequest, resp.Code)

			var errorResp map[string]string
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &errorResp))
			assert.Contains(t, errorResp["error"], "already a member")
		})

		t.Run("JoinCircle_CircleNotFound", func(t *testing.T) {
			router, _, _ := build(t)

			userID := createTestUser(t, sqlDB, "user")

			req := httptest.NewRequest(http.MethodPost, "/protected/circles/99999/join", nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(userID).String())
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusNotFound, resp.Code)
		})

		t.Run("JoinCircle_Unauthorized", func(t *testing.T) {
			router, svc, _ := build(t)

			creatorID := createTestUser(t, sqlDB, "creator")

			ctx := context.Background()
			createdCircle, err := svc.CreateCircle(ctx, "Private Circle", creatorID)
			require.NoError(t, err)

			req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/circles/%d/join", createdCircle.ID), nil)
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusUnauthorized, resp.Code)
		})

		t.Run("JoinCircle_InvalidCircleID", func(t *testing.T) {
			router, _, _ := build(t)

			userID := createTestUser(t, sqlDB, "user")

			req := httptest.NewRequest(http.MethodPost, "/protected/circles/invalid/join", nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(userID).String())
			resp := httptest.NewRecorder()

			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusBadRequest, resp.Code)
		})

		t.Run("GetCircleContests", func(t *testing.T) {
			router, svc, _ := build(t)

			creatorID := createTestUser(t, sqlDB, "creator")
			ctx := context.Background()

			testCircle, err := svc.CreateCircle(ctx, "Test Circle", creatorID)
			require.NoError(t, err)

			contestOptions := []string{"Option A", "Option B"}

			contest, err := svc.CreateContest(ctx, testCircle.ID, creatorID, "Test Question?", contestOptions, contest.Duration1Day, 100)
			require.NoError(t, err)

			req := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d/contests", testCircle.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusOK, resp.Code)

			var contests []contestResponse
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &contests))

			require.Len(t, contests, 1)
			assert.Equal(t, int32(contest.ID), contests[0].ID)
			assert.Equal(t, "Test Question?", contests[0].Question)
			assert.Equal(t, 100, contests[0].MinStake)
			assert.Len(t, contests[0].Options, 2)
		})

		t.Run("GetCircleContests_Empty", func(t *testing.T) {
			router, svc, _ := build(t)

			creatorID := createTestUser(t, sqlDB, "creator")
			ctx := context.Background()

			testCircle, err := svc.CreateCircle(ctx, "Empty Circle", creatorID)
			require.NoError(t, err)

			req := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d/contests", testCircle.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			require.Equal(t, http.StatusOK, resp.Code)

			var contests []contestResponse
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &contests))

			assert.Len(t, contests, 0)
		})

		t.Run("ResolveAndDistributeContestClout", func(t *testing.T) {
			router, svc, _ := build(t)
			ctx := context.Background()

			// 1. Setup Users
			creatorID := createTestUser(t, sqlDB, "creator")
			winnerID := createTestUser(t, sqlDB, "winner")
			loserID := createTestUser(t, sqlDB, "loser")

			// 2. Setup Circle
			circ, err := svc.CreateCircle(ctx, "Betting Circle", creatorID)
			require.NoError(t, err)
			require.NoError(t, svc.AddMember(ctx, circ.ID, winnerID))
			require.NoError(t, svc.AddMember(ctx, circ.ID, loserID))

			// 3. Create Contest
			cont, err := svc.CreateContest(ctx, circ.ID, creatorID, "Q?", []string{"Win", "Lose"}, contest.Duration1Hour, 10)
			require.NoError(t, err)

			// 4. Place Bets
			// Winner bets 100 on Option 1
			require.NoError(t, svc.Predict(ctx, circ.ID, cont.ID, winnerID, 1, 100))
			// Loser bets 100 on Option 2
			require.NoError(t, svc.Predict(ctx, circ.ID, cont.ID, loserID, 2, 100))

			// 5. Resolve Contest (Option 1 wins)
			resolveBody := `{"winning_option_id": 1}`
			req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/circles/%d/contests/%d/resolve-distribute", circ.ID, cont.ID), bytes.NewBufferString(resolveBody))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())
			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)
			require.Equal(t, http.StatusOK, resp.Code)

			// 6. Verify Contest Detail Response (Check House Rake)
			req = httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d/contests/%d", circ.ID, cont.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())
			resp = httptest.NewRecorder()
			router.ServeHTTP(resp, req)
			require.Equal(t, http.StatusOK, resp.Code)

			var contestResp contestResponse
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &contestResp))

			// Total Pot = 200
			// Losing Stakes = 100
			// Burn = 10% of 100 = 10
			assert.Equal(t, 200, contestResp.TotalPot)
			assert.Equal(t, 10, contestResp.HouseRake, "HTTP response should show 10 house rake (10% of losing stake)")

			// 7. Verify Payout Breakdown Response
			req = httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d/contests/%d/payout-breakdown", circ.ID, cont.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())
			resp = httptest.NewRecorder()
			router.ServeHTTP(resp, req)
			require.Equal(t, http.StatusOK, resp.Code)

			var breakdownResp payoutBreakdownResponse
			require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &breakdownResp))

			assert.Equal(t, 200, breakdownResp.TotalPot)
			assert.Equal(t, 10, breakdownResp.HouseRake)
			assert.Equal(t, 190, breakdownResp.DistributablePot)

			// Verify Winner Payout
			require.Len(t, breakdownResp.Winners, 1)
			winnerRecord := breakdownResp.Winners[0]
			assert.Equal(t, uuid.UUID(winnerID).String(), winnerRecord.UserID)
			assert.Equal(t, 190, winnerRecord.Total, "Winner should receive 190 clout")
		})

		t.Run("StreamContestEvents_ValidSetup", func(t *testing.T) {
			_, svc, _ := build(t)

			creatorID := createTestUser(t, sqlDB, "creator_stream")

			circle, err := svc.CreateCircle(context.Background(), "Stream Circle", creatorID)
			require.NoError(t, err)

			contestObj, err := svc.CreateContest(context.Background(), circle.ID, creatorID, "Will it work?", []string{"Yes", "No"}, contest.Duration1Day, 10)
			require.NoError(t, err)

			// Test that contest exists and can be retrieved
			retrievedContest, err := svc.GetContestInCircle(context.Background(), circle.ID, contestObj.ID)
			require.NoError(t, err)
			assert.Equal(t, contestObj.ID, retrievedContest.ID)
			assert.Equal(t, circle.ID, retrievedContest.CircleID)
		})

		t.Run("StreamContestEvents_ContestNotFound", func(t *testing.T) {
			router, svc, _ := build(t)

			creatorID := createTestUser(t, sqlDB, "creator_stream_nf")

			circle, err := svc.CreateCircle(context.Background(), "Stream Circle NF", creatorID)
			require.NoError(t, err)

			req, _ := http.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d/contests/99999/stream", circle.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusNotFound, resp.Code)

			var errorResp map[string]string
			err = json.Unmarshal(resp.Body.Bytes(), &errorResp)
			require.NoError(t, err)
			assert.Equal(t, "contest not found", errorResp["error"])
		})

		t.Run("StreamContestEvents_ContestNotInCircle", func(t *testing.T) {
			router, svc, _ := build(t)

			creatorID := createTestUser(t, sqlDB, "creator_stream_nic")

			circle1, err := svc.CreateCircle(context.Background(), "Circle 1", creatorID)
			require.NoError(t, err)

			circle2, err := svc.CreateCircle(context.Background(), "Circle 2", creatorID)
			require.NoError(t, err)

			contestObj, err := svc.CreateContest(context.Background(), circle2.ID, creatorID, "Question?", []string{"A", "B"}, contest.Duration1Day, 10)
			require.NoError(t, err)

			req, _ := http.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d/contests/%d/stream", circle1.ID, contestObj.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusNotFound, resp.Code)

			var errorResp map[string]string
			err = json.Unmarshal(resp.Body.Bytes(), &errorResp)
			require.NoError(t, err)
			assert.Equal(t, "contest not found", errorResp["error"])
		})

		t.Run("StreamContestEvents_InvalidCircleID", func(t *testing.T) {
			router, _, _ := build(t)

			creatorID := createTestUser(t, sqlDB, "creator_stream_inv")

			req, _ := http.NewRequest(http.MethodGet, "/protected/circles/invalid/contests/1/stream", nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusBadRequest, resp.Code)
		})

		t.Run("StreamContestEvents_InvalidContestID", func(t *testing.T) {
			router, svc, _ := build(t)

			creatorID := createTestUser(t, sqlDB, "creator_stream_invc")

			circle, err := svc.CreateCircle(context.Background(), "Stream Circle INV", creatorID)
			require.NoError(t, err)

			req, _ := http.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d/contests/invalid/stream", circle.ID), nil)
			req.Header.Set(auth.TestUserIDHeader, uuid.UUID(creatorID).String())

			resp := httptest.NewRecorder()
			router.ServeHTTP(resp, req)

			assert.Equal(t, http.StatusBadRequest, resp.Code)
		})
	})
}
