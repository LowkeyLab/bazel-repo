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
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	contestrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	contestservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
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
		UserID int32 `json:"user_id"`
		Clout  int   `json:"clout"`
	} `json:"members"`
}

func setupTestRouter(t *testing.T) (*gin.Engine, *pgxpool.Pool, *service.Service, *db.Queries, *contestservice.Service) {
	t.Helper()
	gin.SetMode(gin.TestMode)

	pool := testutil.SetupTestDB(t)
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	contestRepo := contestrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)
	contestSvc := contestservice.NewService(contestRepo)
	handler := NewHandler(svc, contestSvc)

	r := gin.New()
	authGroup := r.Group("/protected")
	authGroup.Use(auth.TestMiddleware())
	handler.RegisterRoutes(authGroup)

	return r, pool, svc, db.New(pool), contestSvc
}

func createTestUser(t *testing.T, pool *pgxpool.Pool, username string) user.ID {
	return createUserWithRole(t, pool, username, db.UserRoleMember)
}

func createUserWithRole(t *testing.T, pool *pgxpool.Pool, username string, role db.UserRole) user.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)
	passwordHash := "hash"

	result, err := q.CreateUser(ctx, db.CreateUserParams{
		Username:     username,
		PasswordHash: passwordHash,
		Role:         role,
	})
	require.NoError(t, err)

	return user.ID(result.ID)
}

func TestCreateCircleHandler(t *testing.T) {
	router, pool, _, queries, _ := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "alice")

	body := `{"name":"Study Group"}`
	req := httptest.NewRequest(http.MethodPost, "/protected/circles", bytes.NewBufferString(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusCreated, resp.Code)

	var payload circleResponseBody
	require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &payload))
	assert.Equal(t, "Study Group", payload.Name)
	assert.Len(t, payload.Members, 1)
	assert.Equal(t, int32(creatorID), payload.Members[0].UserID)

	ctx := context.Background()
	dbCircle, err := queries.GetCircle(ctx, payload.ID)
	require.NoError(t, err)
	assert.Equal(t, "Study Group", dbCircle.Name)

	dbMember, err := queries.GetCircleMember(ctx, db.GetCircleMemberParams{
		CircleID: payload.ID,
		UserID:   int32(creatorID),
	})
	require.NoError(t, err)
	assert.Equal(t, int32(1000), dbMember.Clout)
}

func TestAddMemberHandler(t *testing.T) {
	router, pool, svc, queries, _ := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "bob")
	joinerID := createTestUser(t, pool, "charlie")

	ctx := context.Background()
	createdCircle, err := svc.CreateCircle(ctx, "Readers", creatorID)
	require.NoError(t, err)

	body := fmt.Sprintf(`{"user_id":%d}`, joinerID)
	req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/circles/%d/members", createdCircle.ID), bytes.NewBufferString(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusCreated, resp.Code)

	memberRecord, err := queries.GetCircleMember(ctx, db.GetCircleMemberParams{
		CircleID: int32(createdCircle.ID),
		UserID:   int32(joinerID),
	})
	require.NoError(t, err)
	assert.Equal(t, int32(1000), memberRecord.Clout)

	getReq := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d", createdCircle.ID), nil)
	getReq.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))
	getResp := httptest.NewRecorder()
	router.ServeHTTP(getResp, getReq)

	require.Equal(t, http.StatusOK, getResp.Code)
	var circlePayload circleResponseBody
	require.NoError(t, json.Unmarshal(getResp.Body.Bytes(), &circlePayload))

	assert.Equal(t, createdCircle.Name, circlePayload.Name)
	assert.Len(t, circlePayload.Members, 2)
}

func TestDeleteCircle_ByCreator(t *testing.T) {
	router, pool, svc, queries, _ := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "creator")

	ctx := context.Background()
	createdCircle, err := svc.CreateCircle(ctx, "Writers", creatorID)
	require.NoError(t, err)

	req := httptest.NewRequest(http.MethodDelete, fmt.Sprintf("/protected/circles/%d", createdCircle.ID), nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusNoContent, resp.Code)
	_, err = queries.GetCircle(ctx, int32(createdCircle.ID))
	assert.ErrorIs(t, err, pgx.ErrNoRows)
}

func TestDeleteCircle_ByAdmin(t *testing.T) {
	router, pool, svc, queries, _ := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "creator")
	adminID := createUserWithRole(t, pool, "admin", db.UserRoleAdmin)

	ctx := context.Background()
	createdCircle, err := svc.CreateCircle(ctx, "Chess", creatorID)
	require.NoError(t, err)

	req := httptest.NewRequest(http.MethodDelete, fmt.Sprintf("/protected/circles/%d", createdCircle.ID), nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", adminID))

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusNoContent, resp.Code)
	_, err = queries.GetCircle(ctx, int32(createdCircle.ID))
	assert.ErrorIs(t, err, pgx.ErrNoRows)
}

func TestDeleteCircle_Forbidden(t *testing.T) {
	router, pool, svc, _, _ := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "creator")
	memberID := createTestUser(t, pool, "member")

	ctx := context.Background()
	createdCircle, err := svc.CreateCircle(ctx, "Gamers", creatorID)
	require.NoError(t, err)

	req := httptest.NewRequest(http.MethodDelete, fmt.Sprintf("/protected/circles/%d", createdCircle.ID), nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", memberID))

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusForbidden, resp.Code)
}

func TestGetCircle_NotFound(t *testing.T) {
	router, pool, _, _, _ := setupTestRouter(t)

	userID := createTestUser(t, pool, "dana")

	req := httptest.NewRequest(http.MethodGet, "/protected/circles/99999", nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", userID))
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	assert.Equal(t, http.StatusNotFound, resp.Code)
}

func TestListUserCircles(t *testing.T) {
	router, pool, svc, _, _ := setupTestRouter(t)

	// Create users and circles
	user1ID := createTestUser(t, pool, "user1")
	user2ID := createTestUser(t, pool, "user2")

	ctx := context.Background()
	circle1, err := svc.CreateCircle(ctx, "Book Club", user1ID)
	require.NoError(t, err)

	_, err = svc.CreateCircle(ctx, "Movie Night", user1ID)
	require.NoError(t, err)

	// Add user2 to circle1
	err = svc.AddMember(ctx, circle1.ID, user2ID)
	require.NoError(t, err)

	// User1 should see both circles
	req := httptest.NewRequest(http.MethodGet, "/protected/circles", nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", user1ID))
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusOK, resp.Code)
	var circles []circleResponseBody
	require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &circles))

	assert.Len(t, circles, 2)

	// Check that both circles are present
	circleNames := map[string]bool{}
	for _, c := range circles {
		circleNames[c.Name] = true
	}
	assert.True(t, circleNames["Book Club"])
	assert.True(t, circleNames["Movie Night"])

	// User2 should only see circle1
	req2 := httptest.NewRequest(http.MethodGet, "/protected/circles", nil)
	req2.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", user2ID))
	resp2 := httptest.NewRecorder()

	router.ServeHTTP(resp2, req2)

	require.Equal(t, http.StatusOK, resp2.Code)
	var circles2 []circleResponseBody
	require.NoError(t, json.Unmarshal(resp2.Body.Bytes(), &circles2))

	assert.Len(t, circles2, 1)
	assert.Equal(t, "Book Club", circles2[0].Name)
}

func TestListUserCircles_Empty(t *testing.T) {
	router, pool, _, _, _ := setupTestRouter(t)

	userID := createTestUser(t, pool, "lonely")

	req := httptest.NewRequest(http.MethodGet, "/protected/circles", nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", userID))
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusOK, resp.Code)
	var circles []circleResponseBody
	require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &circles))

	assert.Len(t, circles, 0)
}

func TestListUserCircles_Unauthorized(t *testing.T) {
	router, _, _, _, _ := setupTestRouter(t)

	// Request without user ID header
	req := httptest.NewRequest(http.MethodGet, "/protected/circles", nil)
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusUnauthorized, resp.Code)
}

func TestJoinCircle_Success(t *testing.T) {
	router, pool, svc, queries, _ := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "creator")
	joinerID := createTestUser(t, pool, "joiner")

	ctx := context.Background()
	createdCircle, err := svc.CreateCircle(ctx, "Open Circle", creatorID)
	require.NoError(t, err)

	// Joiner joins the circle
	req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/circles/%d/join", createdCircle.ID), nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", joinerID))

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusCreated, resp.Code)

	// Verify the joiner is now a member
	memberRecord, err := queries.GetCircleMember(ctx, db.GetCircleMemberParams{
		CircleID: int32(createdCircle.ID),
		UserID:   int32(joinerID),
	})
	require.NoError(t, err)
	assert.Equal(t, int32(1000), memberRecord.Clout)

	// Verify circle has 2 members
	getReq := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d", createdCircle.ID), nil)
	getReq.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))
	getResp := httptest.NewRecorder()
	router.ServeHTTP(getResp, getReq)

	require.Equal(t, http.StatusOK, getResp.Code)
	var circlePayload circleResponseBody
	require.NoError(t, json.Unmarshal(getResp.Body.Bytes(), &circlePayload))

	assert.Equal(t, createdCircle.Name, circlePayload.Name)
	assert.Len(t, circlePayload.Members, 2)
}

func TestJoinCircle_AlreadyMember(t *testing.T) {
	router, pool, svc, _, _ := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "creator")

	ctx := context.Background()
	createdCircle, err := svc.CreateCircle(ctx, "Exclusive Circle", creatorID)
	require.NoError(t, err)

	// Creator tries to join their own circle
	req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/circles/%d/join", createdCircle.ID), nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusBadRequest, resp.Code)

	var errorResp map[string]string
	require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &errorResp))
	assert.Contains(t, errorResp["error"], "already a member")
}

func TestJoinCircle_CircleNotFound(t *testing.T) {
	router, pool, _, _, _ := setupTestRouter(t)

	userID := createTestUser(t, pool, "user")

	req := httptest.NewRequest(http.MethodPost, "/protected/circles/99999/join", nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", userID))
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	assert.Equal(t, http.StatusNotFound, resp.Code)
}

func TestJoinCircle_Unauthorized(t *testing.T) {
	router, pool, svc, _, _ := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "creator")

	ctx := context.Background()
	createdCircle, err := svc.CreateCircle(ctx, "Private Circle", creatorID)
	require.NoError(t, err)

	// Request without user ID header
	req := httptest.NewRequest(http.MethodPost, fmt.Sprintf("/protected/circles/%d/join", createdCircle.ID), nil)
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusUnauthorized, resp.Code)
}

func TestJoinCircle_InvalidCircleID(t *testing.T) {
	router, pool, _, _, _ := setupTestRouter(t)

	userID := createTestUser(t, pool, "user")

	req := httptest.NewRequest(http.MethodPost, "/protected/circles/invalid/join", nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", userID))
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	assert.Equal(t, http.StatusBadRequest, resp.Code)
}

func TestGetCircleContests(t *testing.T) {
	router, pool, svc, _, contestSvc := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "creator")
	ctx := context.Background()

	// Create a testCircle
	testCircle, err := svc.CreateCircle(ctx, "Test Circle", creatorID)
	require.NoError(t, err)

	// Create a contest in the circle
	contestOptions := []string{"Option A", "Option B"}
	expiresAt := time.Now().Add(24 * time.Hour)
	circleIDs := []circle.ID{testCircle.ID}

	contest, err := contestSvc.CreateContest(ctx, circleIDs, creatorID, "Test Question?", contestOptions, expiresAt)
	require.NoError(t, err)

	// Request contests for the circle
	req := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d/contests", testCircle.ID), nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusOK, resp.Code)

	var contests []contestResponse
	require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &contests))

	require.Len(t, contests, 1)
	assert.Equal(t, int32(contest.ID), contests[0].ID)
	assert.Equal(t, "Test Question?", contests[0].Question)
	assert.Len(t, contests[0].Options, 2)
}

func TestGetCircleContests_Empty(t *testing.T) {
	router, pool, svc, _, _ := setupTestRouter(t)

	creatorID := createTestUser(t, pool, "creator")
	ctx := context.Background()

	// Create a testCircle
	testCircle, err := svc.CreateCircle(ctx, "Empty Circle", creatorID)
	require.NoError(t, err)

	// Request contests for the circle (should be empty)
	req := httptest.NewRequest(http.MethodGet, fmt.Sprintf("/protected/circles/%d/contests", testCircle.ID), nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", creatorID))

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusOK, resp.Code)

	var contests []contestResponse
	require.NoError(t, json.Unmarshal(resp.Body.Bytes(), &contests))

	assert.Len(t, contests, 0)
}
