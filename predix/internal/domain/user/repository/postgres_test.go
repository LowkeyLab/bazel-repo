package repository_test

import (
	"context"
	"testing"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
)

func TestPostgresRepository_Save(t *testing.T) {
	pool := testutil.SetupTestDB(t)
	repo := repository.NewPostgres(pool)

	u, err := user.New("alice", "hash1")
	if err != nil {
		t.Fatalf("failed to create user: %v", err)
	}

	err = repo.Save(context.Background(), u)
	if err != nil {
		t.Fatalf("failed to save user: %v", err)
	}
}

func TestPostgresRepository_FindByID(t *testing.T) {
	pool := testutil.SetupTestDB(t)
	repo := repository.NewPostgres(pool)

	// Create and save user
	u, _ := user.New("bob", "hash2")
	err := repo.Save(context.Background(), u)
	if err != nil {
		t.Fatalf("failed to save user: %v", err)
	}

	// Find by ID
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
}

func TestPostgresRepository_FindByUsername(t *testing.T) {
	pool := testutil.SetupTestDB(t)
	repo := repository.NewPostgres(pool)

	// Create and save user
	u, _ := user.New("charlie", "hash3")
	err := repo.Save(context.Background(), u)
	if err != nil {
		t.Fatalf("failed to save user: %v", err)
	}

	// Find by username
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
}
