package rest

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
)

type loginResponseBody struct {
	Token string `json:"token"`
	User  struct {
		ID       int32  `json:"id"`
		Username string `json:"username"`
		Role     string `json:"role"`
	} `json:"user"`
	Error string `json:"error"`
}

func setupRouter(t *testing.T) (*gin.Engine, *auth.Manager, *service.Service) {
	t.Helper()
	gin.SetMode(gin.TestMode)

	pool := testutil.SetupTestDB(t)
	repo := repository.NewPostgres(pool)
	svc := service.NewService(repo)
	tokens := auth.NewManager("test-secret", time.Hour)

	handler := NewHandler(svc, tokens)

	r := gin.New()
	handler.RegisterRoutes(r)

	return r, tokens, svc
}

func TestRegisterSuccess(t *testing.T) {
	router, tokens, _ := setupRouter(t)

	body, _ := json.Marshal(map[string]string{
		"username": "alice",
		"password": "secret",
	})

	req := httptest.NewRequest(http.MethodPost, "/register", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	if resp.Code != http.StatusCreated {
		t.Fatalf("expected 201, got %d", resp.Code)
	}

	var parsed loginResponseBody
	if err := json.NewDecoder(resp.Body).Decode(&parsed); err != nil {
		t.Fatalf("decode response: %v", err)
	}

	if parsed.Token == "" {
		t.Fatalf("expected token to be issued")
	}

	uid, err := tokens.ParseToken(parsed.Token)
	if err != nil {
		t.Fatalf("token parse failed: %v", err)
	}

	if int32(uid) != parsed.User.ID {
		t.Fatalf("token uid %d does not match response user id %d", uid, parsed.User.ID)
	}
	if parsed.User.Username != "alice" {
		t.Fatalf("expected username alice, got %s", parsed.User.Username)
	}
	if parsed.User.Role != "member" {
		t.Fatalf("expected role member, got %s", parsed.User.Role)
	}
}

func TestRegisterDuplicate(t *testing.T) {
	router, _, svc := setupRouter(t)

	if _, err := svc.Register(context.Background(), "alice", "secret"); err != nil {
		t.Fatalf("seed register failed: %v", err)
	}

	body, _ := json.Marshal(map[string]string{
		"username": "alice",
		"password": "secret",
	})

	req := httptest.NewRequest(http.MethodPost, "/register", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	if resp.Code != http.StatusBadRequest {
		t.Fatalf("expected 400, got %d", resp.Code)
	}

	var parsed map[string]string
	if err := json.NewDecoder(resp.Body).Decode(&parsed); err != nil {
		t.Fatalf("decode response: %v", err)
	}

	if parsed["error"] == "" {
		t.Fatalf("expected error message for duplicate username")
	}
}

func TestLoginSuccess(t *testing.T) {
	router, tokens, svc := setupRouter(t)

	created, err := svc.Register(context.Background(), "alice", "secret")
	if err != nil {
		t.Fatalf("seed register failed: %v", err)
	}

	body, _ := json.Marshal(map[string]string{
		"username": "alice",
		"password": "secret",
	})

	req := httptest.NewRequest(http.MethodPost, "/login", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")

	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	if resp.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", resp.Code)
	}

	var parsed loginResponseBody
	if err := json.NewDecoder(resp.Body).Decode(&parsed); err != nil {
		t.Fatalf("decode response: %v", err)
	}

	uid, err := tokens.ParseToken(parsed.Token)
	if err != nil {
		t.Fatalf("token parse failed: %v", err)
	}

	if uid != created.ID {
		t.Fatalf("expected token uid %d, got %d", created.ID, uid)
	}
	if parsed.User.Username != created.Username {
		t.Fatalf("unexpected username: %s", parsed.User.Username)
	}
}

func TestLoginInvalidPassword(t *testing.T) {
	router, _, svc := setupRouter(t)

	if _, err := svc.Register(context.Background(), "alice", "secret"); err != nil {
		t.Fatalf("seed register failed: %v", err)
	}

	body, _ := json.Marshal(map[string]string{
		"username": "alice",
		"password": "wrong",
	})

	req := httptest.NewRequest(http.MethodPost, "/login", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	if resp.Code != http.StatusBadRequest {
		t.Fatalf("expected 400, got %d", resp.Code)
	}

	var parsed map[string]string
	if err := json.NewDecoder(resp.Body).Decode(&parsed); err != nil {
		t.Fatalf("decode response: %v", err)
	}

	if parsed["error"] != "invalid credentials" {
		t.Fatalf("expected invalid credentials message, got %q", parsed["error"])
	}
}

func TestLoginUnknownUser(t *testing.T) {
	router, _, _ := setupRouter(t)

	body, _ := json.Marshal(map[string]string{
		"username": "missing",
		"password": "secret",
	})

	req := httptest.NewRequest(http.MethodPost, "/login", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	resp := httptest.NewRecorder()
	router.ServeHTTP(resp, req)

	if resp.Code != http.StatusBadRequest {
		t.Fatalf("expected 400, got %d", resp.Code)
	}

	var parsed map[string]string
	if err := json.NewDecoder(resp.Body).Decode(&parsed); err != nil {
		t.Fatalf("decode response: %v", err)
	}

	if parsed["error"] != "invalid credentials" {
		t.Fatalf("expected invalid credentials message, got %q", parsed["error"])
	}
}
