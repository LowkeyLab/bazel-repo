package repository_test

import (
	"context"
	"testing"
	"time"

	clockpkg "github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMemoryRepository_SaveContest(t *testing.T) {
	repo := repository.NewMemory()

	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}
	c, err := contest.New(
		clk,
		circle.ID(1),
		user.ID("user-1"),
		"Test question?",
		[]string{"Yes", "No"},
		contest.Duration1Day,
		10,
	)
	require.NoError(t, err)

	// Save should succeed
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)
	assert.NotZero(t, c.ID)
}

func TestMemoryRepository_SaveContestNil(t *testing.T) {
	repo := repository.NewMemory()

	err := repo.Save(context.Background(), nil)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "cannot be nil")
}

func TestMemoryRepository_FindByID(t *testing.T) {
	repo := repository.NewMemory()

	// Create and save contest
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}
	c, err := contest.New(
		clk,
		circle.ID(2),
		user.ID("user-1"),
		"Will it rain?",
		[]string{"Yes", "No", "Maybe"},
		contest.Duration1Day,
		10,
	)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)

	// Find by ID
	found, err := repo.FindByID(context.Background(), c.ID)
	require.NoError(t, err)
	assert.Equal(t, c.ID, found.ID)
	assert.Equal(t, c.Question, found.Question)
	assert.Equal(t, c.CreatorID, found.CreatorID)
	assert.Equal(t, c.Status, found.Status)
	assert.Equal(t, c.CircleID, found.CircleID)
	assert.Len(t, found.Options, 3)
	assert.Empty(t, found.Predictions)
}

func TestMemoryRepository_FindByIDNotFound(t *testing.T) {
	repo := repository.NewMemory()

	_, err := repo.FindByID(context.Background(), 999)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "not found")
}

func TestMemoryRepository_FindByCircleID(t *testing.T) {
	repo := repository.NewMemory()

	// Create contests with different circles
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}
	c1, err := contest.New(
		clk,
		circle.ID(2),
		user.ID("user-1"),
		"Question 1?",
		[]string{"A", "B"},
		contest.Duration1Day,
		10,
	)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c1)
	require.NoError(t, err)

	c2, err := contest.New(
		clk,
		circle.ID(2),
		user.ID("user-1"),
		"Question 2?",
		[]string{"X", "Y"},
		contest.Duration1Day,
		10,
	)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c2)
	require.NoError(t, err)

	c3, err := contest.New(
		clk,
		circle.ID(3),
		user.ID("user-1"),
		"Question 3?",
		[]string{"M", "N"},
		contest.Duration1Day,
		10,
	)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c3)
	require.NoError(t, err)

	// Find by circle ID 2
	contests, err := repo.FindByCircleID(context.Background(), circle.ID(2))
	require.NoError(t, err)
	assert.Len(t, contests, 2)

	// Find by circle ID 3
	contests, err = repo.FindByCircleID(context.Background(), circle.ID(3))
	require.NoError(t, err)
	assert.Len(t, contests, 1)

	// Find by circle ID 1
	contests, err = repo.FindByCircleID(context.Background(), circle.ID(1))
	require.NoError(t, err)
	assert.Len(t, contests, 0)

	// Find by non-existent circle
	contests, err = repo.FindByCircleID(context.Background(), circle.ID(999))
	require.NoError(t, err)
	assert.Empty(t, contests)
}

func TestMemoryRepository_SaveUpdatesContest(t *testing.T) {
	repo := repository.NewMemory()

	// Create and save contest
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}
	c, err := contest.New(
		clk,
		circle.ID(1),
		user.ID("user-1"),
		"Question?",
		[]string{"A", "B"},
		contest.Duration1Day,
		10,
	)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)

	originalID := c.ID

	// Add a prediction
	err = c.Predict(user.ID("user-2"), 1, 100)
	require.NoError(t, err)

	// Save again
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)
	assert.Equal(t, originalID, c.ID)

	// Retrieve and verify prediction was saved
	found, err := repo.FindByID(context.Background(), c.ID)
	require.NoError(t, err)
	assert.Len(t, found.Predictions, 1)
	assert.Equal(t, user.ID("user-2"), found.Predictions[0].UserID)
	assert.Equal(t, 1, found.Predictions[0].OptionID)
	assert.Equal(t, 100, found.Predictions[0].Clout)
}

func TestMemoryRepository_DeepCopy(t *testing.T) {
	repo := repository.NewMemory()

	// Create and save contest
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}
	c, err := contest.New(
		clk,
		circle.ID(1),
		user.ID("user-1"),
		"Question?",
		[]string{"A", "B"},
		contest.Duration1Day,
		10,
	)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)

	// Retrieve and modify
	found1, err := repo.FindByID(context.Background(), c.ID)
	require.NoError(t, err)
	found1.Question = "Modified Question?"

	// Retrieve again and verify original is unchanged
	found, err := repo.FindByID(context.Background(), c.ID)
	require.NoError(t, err)
	assert.Equal(t, "Question?", found.Question)
}

func TestMemoryRepository_SaveResolvedContest(t *testing.T) {
	repo := repository.NewMemory()

	// Create and save contest
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}
	c, err := contest.New(
		clk,
		circle.ID(1),
		user.ID("user-1"),
		"Question?",
		[]string{"A", "B"},
		contest.Duration1Day,
		10,
	)
	require.NoError(t, err)
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)

	// Add predictions
	require.NoError(t, c.Predict(user.ID("user-2"), 1, 100))
	require.NoError(t, c.Predict(user.ID("user-3"), 2, 200))

	// Resolve the contest
	winningOptionID := 1
	err = c.Resolve(winningOptionID)
	require.NoError(t, err)

	// Save the resolved contest
	err = repo.Save(context.Background(), c)
	require.NoError(t, err)

	// Retrieve and verify resolved state
	found, err := repo.FindByID(context.Background(), c.ID)
	require.NoError(t, err)
	assert.Equal(t, contest.StatusResolved, found.Status)
	require.NotNil(t, found.ResultOptionID)
	assert.Equal(t, winningOptionID, *found.ResultOptionID)
	assert.Len(t, found.Predictions, 2)
}

func TestMemoryRepository_Concurrency(t *testing.T) {
	repo := repository.NewMemory()

	// Test concurrent saves
	done := make(chan bool, 10)
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}
	for i := 0; i < 10; i++ {
		go func(idx int) {
			c, _ := contest.New(
				clk,
				circle.ID(idx+1),
				user.ID("user-1"),
				"Question?",
				[]string{"A", "B"},
				contest.Duration1Day,
				10,
			)
			_ = repo.Save(context.Background(), c)
			done <- true
		}(i)
	}

	// Wait for all goroutines
	for i := 0; i < 10; i++ {
		<-done
	}
}
