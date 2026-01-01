package service

import (
	"context"
	"errors"
	"testing"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
)

func TestLoginSuccess(t *testing.T) {
	svc := newPostgresService(t)
	ctx := context.Background()

	created, err := svc.Register(ctx, "alice", "pass123")
	if err != nil {
		t.Fatalf("register returned error: %v", err)
	}

	got, err := svc.Login(ctx, "alice", "pass123")
	if err != nil {
		t.Fatalf("login returned error: %v", err)
	}

	if got.ID != created.ID || got.Username != created.Username {
		t.Fatalf("unexpected user returned: %+v", got)
	}
}

func TestLoginUnknownUser(t *testing.T) {
	svc := newPostgresService(t)
	ctx := context.Background()

	_, err := svc.Login(ctx, "missing", "pass123")
	if err == nil {
		t.Fatalf("expected error for unknown user")
	}
	if err.Error() != "invalid credentials" {
		t.Fatalf("expected invalid credentials error, got %v", err)
	}

	if _, findErr := svc.repo.FindByUsername(ctx, "missing"); findErr == nil || !errors.Is(findErr, repository.ErrNotFound) {
		t.Fatalf("user should not be created on failed login; got err=%v", findErr)
	}
}

func TestLoginWrongPassword(t *testing.T) {
	svc := newPostgresService(t)
	ctx := context.Background()

	if _, err := svc.Register(ctx, "alice", "pass123"); err != nil {
		t.Fatalf("register returned error: %v", err)
	}

	if _, err := svc.Login(ctx, "alice", "badpass"); err == nil || err.Error() != "invalid credentials" {
		t.Fatalf("expected invalid credentials, got %v", err)
	}
}

func TestRegisterDuplicate(t *testing.T) {
	svc := newPostgresService(t)
	ctx := context.Background()

	if _, err := svc.Register(ctx, "alice", "pass123"); err != nil {
		t.Fatalf("first register returned error: %v", err)
	}

	if _, err := svc.Register(ctx, "alice", "pass123"); err == nil {
		t.Fatalf("expected duplicate error")
	} else if err.Error() != "username already exists" {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestRegisterTrimsInput(t *testing.T) {
	svc := newPostgresService(t)
	ctx := context.Background()

	u, err := svc.Register(ctx, "  bob  ", "  secret  ")
	if err != nil {
		t.Fatalf("register returned error: %v", err)
	}

	if u.Username != "bob" {
		t.Fatalf("expected trimmed username 'bob', got %q", u.Username)
	}
	if u.PasswordHash == "  secret  " {
		t.Fatalf("password should be hashed, got raw value")
	}
	if u.Role != user.RoleMember {
		t.Fatalf("expected default role %q, got %q", user.RoleMember, u.Role)
	}
}

func newPostgresService(t *testing.T) *Service {
	t.Helper()

	pool := testutil.SetupTestDB(t)
	return NewService(repository.NewPostgres(pool))
}
