package rest

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
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

func setupTestRouter(t *testing.T) (*gin.Engine, *pgxpool.Pool, *service.Service, *db.Queries) {
	t.Helper()
	gin.SetMode(gin.TestMode)

	pool := testutil.SetupTestDB(t)
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)
	handler := NewHandler(svc)

	r := gin.New()
	authGroup := r.Group("/protected")
	authGroup.Use(auth.TestMiddleware())
	handler.RegisterRoutes(authGroup)

	return r, pool, svc, db.New(pool)
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
	router, pool, _, queries := setupTestRouter(t)

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
	router, pool, svc, queries := setupTestRouter(t)

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
	router, pool, svc, queries := setupTestRouter(t)

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
	router, pool, svc, queries := setupTestRouter(t)

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
	router, pool, svc, _ := setupTestRouter(t)

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
	router, pool, _, _ := setupTestRouter(t)

	userID := createTestUser(t, pool, "dana")

	req := httptest.NewRequest(http.MethodGet, "/protected/circles/99999", nil)
	req.Header.Set(auth.TestUserIDHeader, fmt.Sprintf("%d", userID))
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	assert.Equal(t, http.StatusNotFound, resp.Code)
}
