package service

import (
	"context"
	"database/sql"
	"errors"
	"testing"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
)

func TestUserService(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, db *sql.DB) {
		ctx := context.Background()

		newService := func(t *testing.T) *Service {
			testutil.ResetTables(t, db)
			return NewService(repository.NewPostgres(db))
		}

		t.Run("LoginSuccess", func(t *testing.T) {
			svc := newService(t)

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
		})

		t.Run("LoginUnknownUser", func(t *testing.T) {
			svc := newService(t)

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
		})

		t.Run("LoginWrongPassword", func(t *testing.T) {
			svc := newService(t)

			if _, err := svc.Register(ctx, "alice", "pass123"); err != nil {
				t.Fatalf("register returned error: %v", err)
			}

			if _, err := svc.Login(ctx, "alice", "badpass"); err == nil || err.Error() != "invalid credentials" {
				t.Fatalf("expected invalid credentials, got %v", err)
			}
		})

		t.Run("RegisterDuplicate", func(t *testing.T) {
			svc := newService(t)

			if _, err := svc.Register(ctx, "alice", "pass123"); err != nil {
				t.Fatalf("first register returned error: %v", err)
			}

			if _, err := svc.Register(ctx, "alice", "pass123"); err == nil {
				t.Fatalf("expected duplicate error")
			} else if err.Error() != "username already exists" {
				t.Fatalf("unexpected error: %v", err)
			}
		})

		t.Run("RegisterTrimsInput", func(t *testing.T) {
			svc := newService(t)

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
		})
	})
}
