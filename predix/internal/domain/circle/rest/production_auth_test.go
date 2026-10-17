package rest

import (
	"bytes"
	"context"
	"database/sql"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	sloggin "github.com/gin-contrib/slog"
	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	circlerepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/sse"
	contestrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	userrest "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/rest"
	userservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestProductionAuthentication(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, sqlDB *sql.DB) {
		for _, tc := range []struct {
			name  string
			login bool
		}{
			{name: "login token creates circle owned by authenticated member", login: true},
			{name: "test identity header alone cannot create circle"},
		} {
			t.Run(tc.name, func(t *testing.T) {
				testutil.ResetTables(t, sqlDB)
				ctx := context.Background()
				users := userservice.NewService(userrepo.NewPostgres(sqlDB))
				alice, err := users.Register(ctx, "alice", "fixture-password")
				require.NoError(t, err)
				bob, err := users.Register(ctx, "bob", "fixture-password")
				require.NoError(t, err)
				newCircleService := func() *service.Service {
					return service.NewService(circlerepo.NewPostgres(sqlDB), userrepo.NewPostgres(sqlDB), contestrepo.NewPostgres(sqlDB), clock.RealClock{}, &TestTxManager{sqlDB: sqlDB}, sse.NewManager())
				}
				tokens := auth.NewManager("fixture-only-secret", time.Hour)
				router := gin.New()
				router.Use(sloggin.SetLogger())
				userrest.NewHandler(users, tokens).RegisterRoutes(router)
				protected := router.Group("/protected", tokens.Middleware())
				NewHandler(newCircleService(), sse.NewManager()).RegisterRoutes(protected)

				create := httptest.NewRequest(http.MethodPost, "/protected/circles", bytes.NewBufferString(`{"name":"Authenticated circle"}`))
				create.Header.Set("Content-Type", "application/json")
				// A conflicting test identity must not override the production token.
				create.Header.Set(auth.TestUserIDHeader, uuid.UUID(bob.ID).String())
				if tc.login {
					login := httptest.NewRequest(http.MethodPost, "/login", bytes.NewBufferString(`{"username":"alice","password":"fixture-password"}`))
					login.Header.Set("Content-Type", "application/json")
					response := httptest.NewRecorder()
					router.ServeHTTP(response, login)
					require.Equal(t, http.StatusOK, response.Code, response.Body.String())
					var issued struct {
						Token string `json:"token"`
						User  struct {
							ID string `json:"id"`
						} `json:"user"`
					}
					require.NoError(t, json.Unmarshal(response.Body.Bytes(), &issued))
					require.NotEmpty(t, issued.Token)
					require.Equal(t, uuid.UUID(alice.ID).String(), issued.User.ID)
					create.Header.Set("Authorization", "Bearer "+issued.Token)
				}
				response := httptest.NewRecorder()
				router.ServeHTTP(response, create)

				// Reconstruct the service: observe durable work through its public interface.
				reader := newCircleService()
				aliceCircles, err := reader.ListUserCircles(ctx, alice.ID)
				require.NoError(t, err)
				bobCircles, err := reader.ListUserCircles(ctx, bob.ID)
				require.NoError(t, err)
				assert.Empty(t, bobCircles, "test identity header must not authorize database work")
				if !tc.login {
					assert.Equal(t, http.StatusUnauthorized, response.Code)
					assert.JSONEq(t, `{"error":"authorization required"}`, response.Body.String())
					assert.Empty(t, aliceCircles)
					return
				}
				require.Equal(t, http.StatusCreated, response.Code, response.Body.String())
				var created circleResponseBody
				require.NoError(t, json.Unmarshal(response.Body.Bytes(), &created))
				require.Len(t, aliceCircles, 1)
				assert.Equal(t, created.ID, int32(aliceCircles[0].ID))
				assert.Equal(t, "Authenticated circle", aliceCircles[0].Name)
				assert.Equal(t, alice.ID, aliceCircles[0].CreatorID)
				require.Len(t, created.Members, 1)
				assert.Equal(t, uuid.UUID(alice.ID).String(), created.Members[0].UserID)
			})
		}
	})
}
