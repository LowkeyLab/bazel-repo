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
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// createTestUser creates a test user in the database
func createTestUser(t *testing.T, pool *pgxpool.Pool, username string) user.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)
	passwordHash := "hash"

	result, err := q.CreateUser(ctx, db.CreateUserParams{
		Username:     username,
		PasswordHash: passwordHash,
		Role:         db.UserRoleMember,
	})
	require.NoError(t, err)

	return user.ID(result.ID)
}

// createTestCircle creates a test circle in the database
func createTestCircle(t *testing.T, pool *pgxpool.Pool, name string, creatorID user.ID) circle.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)

	result, err := q.CreateCircle(ctx, db.CreateCircleParams{
		Name:      name,
		CreatorID: int32(creatorID),
		CreatedAt: pgtype.Timestamp{Time: time.Now(), Valid: true},
	})
	require.NoError(t, err)

	return circle.ID(result.ID)
}

func TestContestService(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, pool *pgxpool.Pool) {
		ctx := context.Background()

		build := func(t *testing.T) (*service.Service, *db.Queries) {
			testutil.ResetTables(t, pool)
			repo := repository.NewPostgres(pool)
			return service.NewService(repo), db.New(pool)
		}

		t.Run("CreateContest", func(t *testing.T) {
			svc, q := build(t)

			creatorID := createTestUser(t, pool, "alice")
			circleID := createTestCircle(t, pool, "Book Club", creatorID)
			otherCircleID := createTestCircle(t, pool, "New Ideas", creatorID)

			opts := []string{"Yes", "No", "Maybe"}
			expiresAt := time.Now().Add(24 * time.Hour)

			c, err := svc.CreateContest(ctx, []circle.ID{circleID, otherCircleID}, creatorID, "Should we read this book?", opts, expiresAt, 0)
			require.NoError(t, err)
			assert.NotNil(t, c)
			assert.Equal(t, "Should we read this book?", c.Question)
			assert.Equal(t, contest.StatusOpen, c.Status)
			assert.Equal(t, 10, c.MinStake)
			assert.Len(t, c.Options, 3)
			assert.ElementsMatch(t, []circle.ID{circleID, otherCircleID}, c.CircleIDs)

			dbContest, err := q.GetContest(ctx, int32(c.ID))
			require.NoError(t, err)
			assert.Equal(t, "Should we read this book?", dbContest.Question)
			assert.Equal(t, string(contest.StatusOpen), dbContest.Status)
			assert.Equal(t, int32(10), dbContest.MinStake)

			circleLinks, err := q.ListContestCircles(ctx, int32(c.ID))
			require.NoError(t, err)
			assert.Len(t, circleLinks, 2)

			dbOptions, err := q.ListContestOptions(ctx, int32(c.ID))
			require.NoError(t, err)
			assert.Len(t, dbOptions, 3)
		})

		t.Run("CreateContest_WithCustomMinStake", func(t *testing.T) {
			svc, q := build(t)

			creatorID := createTestUser(t, pool, "iris")
			circleID := createTestCircle(t, pool, "Poker Night", creatorID)

			opts := []string{"Go all in", "Fold"}
			expiresAt := time.Now().Add(12 * time.Hour)

			c, err := svc.CreateContest(ctx, []circle.ID{circleID}, creatorID, "What will the dealer show?", opts, expiresAt, 100)
			require.NoError(t, err)
			assert.Equal(t, 100, c.MinStake)

			dbContest, err := q.GetContest(ctx, int32(c.ID))
			require.NoError(t, err)
			assert.Equal(t, int32(100), dbContest.MinStake)
		})

		t.Run("CreateContest_WithInvalidData", func(t *testing.T) {
			svc, _ := build(t)

			creatorID := createTestUser(t, pool, "bob")
			circleID := createTestCircle(t, pool, "Test Circle", creatorID)

			t.Run("empty question", func(t *testing.T) {
				opts := []string{"Yes", "No"}
				expiresAt := time.Now().Add(24 * time.Hour)

				c, err := svc.CreateContest(ctx, []circle.ID{circleID}, creatorID, "", opts, expiresAt, 0)
				assert.Error(t, err)
				assert.Nil(t, c)
				assert.Contains(t, err.Error(), "question cannot be empty")
			})

			t.Run("not enough options", func(t *testing.T) {
				opts := []string{"Yes"}
				expiresAt := time.Now().Add(24 * time.Hour)

				c, err := svc.CreateContest(ctx, []circle.ID{circleID}, creatorID, "Valid question?", opts, expiresAt, 0)
				assert.Error(t, err)
				assert.Nil(t, c)
				assert.Contains(t, err.Error(), "at least two options")
			})

			t.Run("empty option text", func(t *testing.T) {
				opts := []string{"Yes", ""}
				expiresAt := time.Now().Add(24 * time.Hour)

				c, err := svc.CreateContest(ctx, []circle.ID{circleID}, creatorID, "Valid question?", opts, expiresAt, 0)
				assert.Error(t, err)
				assert.Nil(t, c)
				assert.Contains(t, err.Error(), "option text cannot be empty")
			})

			t.Run("duplicate options", func(t *testing.T) {
				opts := []string{"Yes", "No", "Yes"}
				expiresAt := time.Now().Add(24 * time.Hour)

				c, err := svc.CreateContest(ctx, []circle.ID{circleID}, creatorID, "Valid question?", opts, expiresAt, 0)
				assert.Error(t, err)
				assert.Nil(t, c)
				assert.Contains(t, err.Error(), "duplicate options")
			})

			t.Run("no circles", func(t *testing.T) {
				opts := []string{"Yes", "No"}
				expiresAt := time.Now().Add(24 * time.Hour)

				c, err := svc.CreateContest(ctx, []circle.ID{}, creatorID, "Valid question?", opts, expiresAt, 0)
				assert.Error(t, err)
				assert.Nil(t, c)
				assert.Contains(t, err.Error(), "at least one circle")
			})
		})

		t.Run("Predict", func(t *testing.T) {
			svc, q := build(t)

			userID := createTestUser(t, pool, "Charlie")
			circleID := createTestCircle(t, pool, "Predictions Circle", userID)

			opts := []string{"Option A", "Option B"}
			expiresAt := time.Now().Add(24 * time.Hour)
			c, err := svc.CreateContest(ctx, []circle.ID{circleID}, userID, "What will happen?", opts, expiresAt, 0)
			require.NoError(t, err)

			err = svc.Predict(ctx, c.ID, userID, 1, 100)
			require.NoError(t, err)

			predictions, err := q.ListContestPredictions(ctx, int32(c.ID))
			require.NoError(t, err)
			assert.Len(t, predictions, 1)
			assert.Equal(t, int32(userID), predictions[0].UserID)
			assert.Equal(t, int32(1), predictions[0].OptionID)
			assert.Equal(t, int32(100), predictions[0].Clout)
		})

		t.Run("Predict_ContestNotOpen", func(t *testing.T) {
			svc, q := build(t)

			userID := createTestUser(t, pool, "Diana")
			circleID := createTestCircle(t, pool, "Test Circle", userID)

			opts := []string{"Option A", "Option B"}
			expiresAt := time.Now().Add(24 * time.Hour)
			c, err := svc.CreateContest(ctx, []circle.ID{circleID}, userID, "Test question?", opts, expiresAt, 0)
			require.NoError(t, err)

			err = q.UpdateContestStatus(ctx, db.UpdateContestStatusParams{
				ID:             int32(c.ID),
				Status:         "CLOSED",
				ResultOptionID: pgtype.Int4{Valid: false},
			})
			require.NoError(t, err)

			err = svc.Predict(ctx, c.ID, userID, 1, 100)
			assert.Error(t, err)
			assert.Contains(t, err.Error(), "not open")
		})

		t.Run("Predict_BelowMinStake", func(t *testing.T) {
			svc, _ := build(t)

			userID := createTestUser(t, pool, "Isabel")
			circleID := createTestCircle(t, pool, "High Rollers", userID)

			opts := []string{"Heads", "Tails"}
			expiresAt := time.Now().Add(24 * time.Hour)
			c, err := svc.CreateContest(ctx, []circle.ID{circleID}, userID, "Coin flip?", opts, expiresAt, 100)
			require.NoError(t, err)

			err = svc.Predict(ctx, c.ID, userID, 1, 50)
			assert.Error(t, err)
			assert.Contains(t, err.Error(), "at least 100")
		})

		t.Run("ResolveContest", func(t *testing.T) {
			svc, q := build(t)

			userID := createTestUser(t, pool, "Eve")
			circleID := createTestCircle(t, pool, "Resolution Circle", userID)

			opts := []string{"Outcome A", "Outcome B"}
			expiresAt := time.Now().Add(24 * time.Hour)
			c, err := svc.CreateContest(ctx, []circle.ID{circleID}, userID, "What is the outcome?", opts, expiresAt, 0)
			require.NoError(t, err)

			err = svc.ResolveContest(ctx, c.ID, userID, 1)
			require.NoError(t, err)

			dbContest, err := q.GetContest(ctx, int32(c.ID))
			require.NoError(t, err)
			assert.Equal(t, "RESOLVED", dbContest.Status)
			assert.True(t, dbContest.ResultOptionID.Valid)
			assert.Equal(t, int32(1), dbContest.ResultOptionID.Int32)
		})

		t.Run("ResolveContest_OnlyCreatorCanResolve", func(t *testing.T) {
			svc, q := build(t)

			creatorID := createTestUser(t, pool, "Grace")
			nonCreatorID := createTestUser(t, pool, "Henry")
			circleID := createTestCircle(t, pool, "Ownership Circle", creatorID)

			opts := []string{"Outcome A", "Outcome B"}
			expiresAt := time.Now().Add(24 * time.Hour)
			c, err := svc.CreateContest(ctx, []circle.ID{circleID}, creatorID, "Which team wins?", opts, expiresAt, 0)
			require.NoError(t, err)

			err = svc.ResolveContest(ctx, c.ID, nonCreatorID, 1)
			assert.ErrorIs(t, err, service.ErrNotContestCreator)

			dbContest, err := q.GetContest(ctx, int32(c.ID))
			require.NoError(t, err)
			assert.Equal(t, string(contest.StatusOpen), dbContest.Status)
			assert.False(t, dbContest.ResultOptionID.Valid)
		})
	})
}
