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

	u, err := user.New("Alice", "alice@example.com")
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
	u, _ := user.New("Bob", "bob@example.com")
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
	if found.Name != u.Name {
		t.Errorf("expected name %v, got %v", u.Name, found.Name)
	}
	if found.Email != u.Email {
		t.Errorf("expected email %v, got %v", u.Email, found.Email)
	}
}

func TestPostgresRepository_FindByEmail(t *testing.T) {
	pool := testutil.SetupTestDB(t)
	repo := repository.NewPostgres(pool)

	// Create and save user
	u, _ := user.New("Charlie", "charlie@example.com")
	err := repo.Save(context.Background(), u)
	if err != nil {
		t.Fatalf("failed to save user: %v", err)
	}

	// Find by email
	found, err := repo.FindByEmail(context.Background(), u.Email)
	if err != nil {
		t.Fatalf("failed to find user by email: %v", err)
	}

	if found.ID != u.ID {
		t.Errorf("expected ID %v, got %v", u.ID, found.ID)
	}
	if found.Email != u.Email {
		t.Errorf("expected email %v, got %v", u.Email, found.Email)
	}
}
