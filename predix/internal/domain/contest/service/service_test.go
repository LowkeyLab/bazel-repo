package service_test

import (
	"context"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// createTestUser creates a test user in the database
func createTestUser(t *testing.T, pool *pgxpool.Pool, name, email string) user.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)

	result, err := q.CreateUser(ctx, db.CreateUserParams{
		Name:  name,
		Email: email,
	})
	require.NoError(t, err)

	return user.ID(result.ID)
}

// createTestCircle creates a test circle in the database
func createTestCircle(t *testing.T, pool *pgxpool.Pool, name, inviteCode string) circle.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)

	result, err := q.CreateCircle(ctx, db.CreateCircleParams{
		Name:       name,
		InviteCode: inviteCode,
		CreatedAt:  pgtype.Timestamp{Time: time.Now(), Valid: true},
	})
	require.NoError(t, err)

	return circle.ID(result.ID)
}

func TestCreateContest(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	svc := service.NewService(pool)
	q := db.New(pool)

	// Setup: Create user and circle
	creatorID := createTestUser(t, pool, "Alice", "alice@example.com")
	circleID := createTestCircle(t, pool, "Book Club", "BOOK01")

	// Test: Create a contest
	opts := []string{"Yes", "No", "Maybe"}
	expiresAt := time.Now().Add(24 * time.Hour)

	c, err := svc.CreateContest(ctx, circleID, creatorID, "Should we read this book?", opts, expiresAt)
	require.NoError(t, err)
	assert.NotNil(t, c)
	assert.Equal(t, "Should we read this book?", c.Question)
	assert.Equal(t, contest.StatusOpen, c.Status)
	assert.Len(t, c.Options, 3)

	// Verify in database
	dbContest, err := q.GetContest(ctx, int32(c.ID))
	require.NoError(t, err)
	assert.Equal(t, "Should we read this book?", dbContest.Question)
	assert.Equal(t, string(contest.StatusOpen), dbContest.Status)

	// Verify options
	dbOptions, err := q.ListContestOptions(ctx, int32(c.ID))
	require.NoError(t, err)
	assert.Len(t, dbOptions, 3)
}

func TestCreateContest_WithInvalidData(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	svc := service.NewService(pool)

	creatorID := createTestUser(t, pool, "Bob", "bob@example.com")
	circleID := createTestCircle(t, pool, "Test Circle", "TEST01")

	t.Run("empty question", func(t *testing.T) {
		opts := []string{"Yes", "No"}
		expiresAt := time.Now().Add(24 * time.Hour)

		c, err := svc.CreateContest(ctx, circleID, creatorID, "", opts, expiresAt)
		assert.Error(t, err)
		assert.Nil(t, c)
		assert.Contains(t, err.Error(), "question cannot be empty")
	})

	t.Run("not enough options", func(t *testing.T) {
		opts := []string{"Yes"}
		expiresAt := time.Now().Add(24 * time.Hour)

		c, err := svc.CreateContest(ctx, circleID, creatorID, "Valid question?", opts, expiresAt)
		assert.Error(t, err)
		assert.Nil(t, c)
		assert.Contains(t, err.Error(), "at least two options")
	})

	t.Run("empty option text", func(t *testing.T) {
		opts := []string{"Yes", ""}
		expiresAt := time.Now().Add(24 * time.Hour)

		c, err := svc.CreateContest(ctx, circleID, creatorID, "Valid question?", opts, expiresAt)
		assert.Error(t, err)
		assert.Nil(t, c)
		assert.Contains(t, err.Error(), "option text cannot be empty")
	})

	t.Run("duplicate options", func(t *testing.T) {
		opts := []string{"Yes", "No", "Yes"}
		expiresAt := time.Now().Add(24 * time.Hour)

		c, err := svc.CreateContest(ctx, circleID, creatorID, "Valid question?", opts, expiresAt)
		assert.Error(t, err)
		assert.Nil(t, c)
		assert.Contains(t, err.Error(), "duplicate options")
	})
}

func TestPredict(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	svc := service.NewService(pool)
	q := db.New(pool)

	// Setup: Create user, circle, and contest
	userID := createTestUser(t, pool, "Charlie", "charlie@example.com")
	circleID := createTestCircle(t, pool, "Predictions Circle", "PRED01")

	opts := []string{"Option A", "Option B"}
	expiresAt := time.Now().Add(24 * time.Hour)
	c, err := svc.CreateContest(ctx, circleID, userID, "What will happen?", opts, expiresAt)
	require.NoError(t, err)

	// Test: Make a prediction (option IDs start at 1)
	err = svc.Predict(ctx, c.ID, userID, 1, 100)
	require.NoError(t, err)

	// Verify prediction in database
	predictions, err := q.ListContestPredictions(ctx, int32(c.ID))
	require.NoError(t, err)
	assert.Len(t, predictions, 1)
	assert.Equal(t, int32(userID), predictions[0].UserID)
	assert.Equal(t, int32(1), predictions[0].OptionID)
	assert.Equal(t, int32(100), predictions[0].Clout)
}

func TestPredict_ContestNotOpen(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	svc := service.NewService(pool)
	q := db.New(pool)

	// Setup: Create user, circle, and contest
	userID := createTestUser(t, pool, "Diana", "diana@example.com")
	circleID := createTestCircle(t, pool, "Test Circle", "TEST02")

	opts := []string{"Option A", "Option B"}
	expiresAt := time.Now().Add(24 * time.Hour)
	c, err := svc.CreateContest(ctx, circleID, userID, "Test question?", opts, expiresAt)
	require.NoError(t, err)

	// Manually close the contest
	err = q.UpdateContestStatus(ctx, db.UpdateContestStatusParams{
		ID:             int32(c.ID),
		Status:         "CLOSED",
		ResultOptionID: pgtype.Int4{Valid: false},
	})
	require.NoError(t, err)

	// Test: Try to predict on closed contest
	err = svc.Predict(ctx, c.ID, userID, 1, 100)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "not open")
}

func TestResolveContest(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	svc := service.NewService(pool)
	q := db.New(pool)

	// Setup: Create user, circle, and contest
	userID := createTestUser(t, pool, "Eve", "eve@example.com")
	circleID := createTestCircle(t, pool, "Resolution Circle", "RSOL01")

	opts := []string{"Outcome A", "Outcome B"}
	expiresAt := time.Now().Add(24 * time.Hour)
	c, err := svc.CreateContest(ctx, circleID, userID, "What is the outcome?", opts, expiresAt)
	require.NoError(t, err)

	// Test: Resolve the contest
	err = svc.ResolveContest(ctx, c.ID, 1)
	require.NoError(t, err)

	// Verify in database
	dbContest, err := q.GetContest(ctx, int32(c.ID))
	require.NoError(t, err)
	assert.Equal(t, "RESOLVED", dbContest.Status)
	assert.True(t, dbContest.ResultOptionID.Valid)
	assert.Equal(t, int32(1), dbContest.ResultOptionID.Int32)
}
