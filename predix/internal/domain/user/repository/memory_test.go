package repository_test

import (
	"context"
	"testing"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMemoryRepository_Save(t *testing.T) {
	repo := repository.NewMemory()

	u, err := user.New("alice", "hash1")
	require.NoError(t, err)

	// Save should succeed
	err = repo.Save(context.Background(), u)
	require.NoError(t, err)
	assert.NotZero(t, u.ID)

	// Save with duplicate username should fail
	u2, err := user.New("alice", "hash2")
	require.NoError(t, err)
	err = repo.Save(context.Background(), u2)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "already exists")
}

func TestMemoryRepository_SaveNil(t *testing.T) {
	repo := repository.NewMemory()

	err := repo.Save(context.Background(), nil)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "cannot be nil")
}

func TestMemoryRepository_FindByID(t *testing.T) {
	repo := repository.NewMemory()

	// Create and save user
	u, err := user.New("bob", "hash")
	require.NoError(t, err)
	err = repo.Save(context.Background(), u)
	require.NoError(t, err)

	// Find by ID
	found, err := repo.FindByID(context.Background(), u.ID)
	require.NoError(t, err)
	assert.Equal(t, u.ID, found.ID)
	assert.Equal(t, u.Username, found.Username)
	assert.Equal(t, u.PasswordHash, found.PasswordHash)
}

func TestMemoryRepository_FindByIDNotFound(t *testing.T) {
	repo := repository.NewMemory()

	_, err := repo.FindByID(context.Background(), 999)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "not found")
}

func TestMemoryRepository_FindByUsername(t *testing.T) {
	repo := repository.NewMemory()

	// Create and save user
	u, err := user.New("charlie", "hash")
	require.NoError(t, err)
	err = repo.Save(context.Background(), u)
	require.NoError(t, err)

	// Find by username
	found, err := repo.FindByUsername(context.Background(), u.Username)
	require.NoError(t, err)
	assert.Equal(t, u.ID, found.ID)
	assert.Equal(t, u.Username, found.Username)
}

func TestMemoryRepository_FindByUsernameNotFound(t *testing.T) {
	repo := repository.NewMemory()

	_, err := repo.FindByUsername(context.Background(), "missing")
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "not found")
}

func TestMemoryRepository_Concurrency(t *testing.T) {
	repo := repository.NewMemory()

	// Test concurrent saves
	done := make(chan bool, 10)
	for i := 0; i < 10; i++ {
		go func(idx int) {
			u, _ := user.New("user"+string(rune('0'+idx)), "hash")
			_ = repo.Save(context.Background(), u)
			done <- true
		}(i)
	}

	// Wait for all goroutines
	for i := 0; i < 10; i++ {
		<-done
	}
}
