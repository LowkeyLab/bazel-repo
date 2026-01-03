package repository_test

import (
	"context"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
)

func TestPostgresRepository(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, pool *pgxpool.Pool) {
		setup := func(t *testing.T) *repository.Postgres {
			testutil.ResetTables(t, pool)
			return repository.NewPostgres(pool)
		}

		t.Run("Save", func(t *testing.T) {
			repo := setup(t)

			u, err := user.New("alice", "hash1")
			if err != nil {
				t.Fatalf("failed to create user: %v", err)
			}

			err = repo.Save(context.Background(), u)
			if err != nil {
				t.Fatalf("failed to save user: %v", err)
			}
		})

		t.Run("FindByID", func(t *testing.T) {
			repo := setup(t)

			u, _ := user.New("bob", "hash2")
			if err := repo.Save(context.Background(), u); err != nil {
				t.Fatalf("failed to save user: %v", err)
			}

			found, err := repo.FindByID(context.Background(), u.ID)
			if err != nil {
				t.Fatalf("failed to find user: %v", err)
			}

			if found.ID != u.ID {
				t.Errorf("expected ID %v, got %v", u.ID, found.ID)
			}
			if found.Username != u.Username {
				t.Errorf("expected username %v, got %v", u.Username, found.Username)
			}
			if found.Role != u.Role {
				t.Errorf("expected role %v, got %v", u.Role, found.Role)
			}
		})

		t.Run("FindByUsername", func(t *testing.T) {
			repo := setup(t)

			u, _ := user.New("charlie", "hash3")
			if err := repo.Save(context.Background(), u); err != nil {
				t.Fatalf("failed to save user: %v", err)
			}

			found, err := repo.FindByUsername(context.Background(), u.Username)
			if err != nil {
				t.Fatalf("failed to find user by username: %v", err)
			}

			if found.ID != u.ID {
				t.Errorf("expected ID %v, got %v", u.ID, found.ID)
			}
			if found.Username != u.Username {
				t.Errorf("expected username %v, got %v", u.Username, found.Username)
			}
			if found.Role != u.Role {
				t.Errorf("expected role %v, got %v", u.Role, found.Role)
			}
		})
	})
}
