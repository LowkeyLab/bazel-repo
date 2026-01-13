package repository_test

import (
	"context"
	"testing"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMemoryRepository_SaveCircle(t *testing.T) {
	repo := repository.NewMemory()

	c, err := circle.New("Book Club", user.ID(uuid.New()))
	require.NoError(t, err)

	// Save should succeed
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)
	assert.NotZero(t, c.ID)
}

func TestMemoryRepository_SaveCircleNil(t *testing.T) {
	repo := repository.NewMemory()

	err := repo.Save(context.Background(), nil)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "cannot be nil")
}

func TestMemoryRepository_FindByID(t *testing.T) {
	repo := repository.NewMemory()

	creatorID := user.ID(uuid.New())
	// Create and save circle
	c, err := circle.New("Book Club", creatorID)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)

	// Find by ID
	found, err := repo.FindByID(context.Background(), c.ID)
	require.NoError(t, err)
	assert.Equal(t, c.ID, found.ID)
	assert.Equal(t, c.Name, found.Name)
	assert.Len(t, found.Members, 1)
	assert.Contains(t, found.Members, creatorID)
}

func TestMemoryRepository_FindByIDNotFound(t *testing.T) {
	repo := repository.NewMemory()

	_, err := repo.FindByID(context.Background(), 999)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "not found")
}

func TestMemoryRepository_FindByUserID(t *testing.T) {
	repo := repository.NewMemory()

	user1ID := user.ID(uuid.New())
	user2ID := user.ID(uuid.New())
	user3ID := user.ID(uuid.New())

	// Create and save multiple circles
	c1, err := circle.New("Book Club", user1ID)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c1)
	require.NoError(t, err)

	c2, err := circle.New("Movie Club", user1ID)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c2)
	require.NoError(t, err)

	// Add user 2 to circle 1
	member := &circle.Member{
		UserID: user2ID,
		Clout:  100,
	}
	err = repo.AddMember(context.Background(), c1.ID, member)
	require.NoError(t, err)

	// User 1 should have 2 circles
	circles1, err := repo.FindByUserID(context.Background(), user1ID)
	require.NoError(t, err)
	assert.Len(t, circles1, 2)

	// User 2 should have 1 circle
	circles2, err := repo.FindByUserID(context.Background(), user2ID)
	require.NoError(t, err)
	assert.Len(t, circles2, 1)
	assert.Equal(t, c1.ID, circles2[0].ID)

	// User 3 should have 0 circles
	circles3, err := repo.FindByUserID(context.Background(), user3ID)
	require.NoError(t, err)
	assert.Len(t, circles3, 0)
}

func TestMemoryRepository_AddMember(t *testing.T) {
	repo := repository.NewMemory()

	creatorID := user.ID(uuid.New())
	// Create and save circle
	c, err := circle.New("Test Circle", creatorID)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)

	newUserID := user.ID(uuid.New())
	// Add a member
	member := &circle.Member{
		UserID: newUserID,
		Clout:  100,
	}
	err = repo.AddMember(context.Background(), c.ID, member)
	require.NoError(t, err)

	// Verify member was added
	found, err := repo.FindByID(context.Background(), c.ID)
	require.NoError(t, err)
	assert.Len(t, found.Members, 2)
	assert.Contains(t, found.Members, newUserID)
	assert.Equal(t, 100, found.Members[newUserID].Clout)
}

func TestMemoryRepository_AddMemberNil(t *testing.T) {
	repo := repository.NewMemory()

	c, err := circle.New("Test Circle", user.ID(uuid.New()))
	require.NoError(t, err)
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)

	err = repo.AddMember(context.Background(), c.ID, nil)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "cannot be nil")
}

func TestMemoryRepository_AddMemberToNonExistentCircle(t *testing.T) {
	repo := repository.NewMemory()

	member := &circle.Member{
		UserID: user.ID(uuid.New()),
		Clout:  100,
	}
	err := repo.AddMember(context.Background(), 999, member)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "not found")
}

func TestMemoryRepository_DeepCopy(t *testing.T) {
	repo := repository.NewMemory()

	// Create and save circle
	c, err := circle.New("Test Circle", user.ID(uuid.New()))
	require.NoError(t, err)
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)

	// Retrieve and modify
	found1, err := repo.FindByID(context.Background(), c.ID)
	require.NoError(t, err)
	found1.Name = "Modified Name"

	// Retrieve again and verify original is unchanged
	found2, err := repo.FindByID(context.Background(), c.ID)
	require.NoError(t, err)
	assert.Equal(t, "Test Circle", found2.Name)
}

func TestMemoryRepository_Concurrency(t *testing.T) {
	repo := repository.NewMemory()

	// Test concurrent saves
	done := make(chan bool, 10)
	for i := 0; i < 10; i++ {
		go func(idx int) {
			c, _ := circle.New("Circle", user.ID(uuid.New()))
			_ = repo.Save(context.Background(), c)
			done <- true
		}(i)
	}

	// Wait for all goroutines
	for i := 0; i < 10; i++ {
		<-done
	}
}
