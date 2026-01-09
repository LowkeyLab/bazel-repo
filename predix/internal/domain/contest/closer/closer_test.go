package closer_test

import (
	"context"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/closer"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func createPrerequisites(t *testing.T, pool *pgxpool.Pool) (circle.ID, user.ID) {
	ctx := context.Background()
	q := db.New(pool)

	userID := user.ID("test-user-id")

	// Create Circle
	c, err := q.CreateCircle(ctx, db.CreateCircleParams{
		Name:      "Test Circle",
		CreatorID: string(userID),
		CreatedAt: pgtype.Timestamp{Time: time.Now(), Valid: true},
	})
	require.NoError(t, err)

	return circle.ID(c.ID), userID
}

func TestCloser(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, pool *pgxpool.Pool) {
		setup := func(t *testing.T) *repository.Postgres {
			testutil.ResetTables(t, pool)
			return repository.NewPostgres(pool)
		}

		t.Run("CloseExpiredContests_ClosesOnlyExpired", func(t *testing.T) {
			repo := setup(t)
			circleID, userID := createPrerequisites(t, pool)

			now := time.Date(2026, 1, 5, 12, 0, 0, 0, time.UTC)

			// Create expired contest
			expiredClock := clock.FixedClock{Time: now.Add(-2 * time.Hour)}
			expired, err := contest.New(expiredClock, circleID, userID, "Expired?", []string{"Yes", "No"}, contest.Duration1Hour, 10)
			require.NoError(t, err)
			require.NoError(t, repo.Save(context.Background(), expired))

			// Create active contest
			activeClock := clock.FixedClock{Time: now}
			active, err := contest.New(activeClock, circleID, userID, "Active?", []string{"A", "B"}, contest.Duration1Hour, 10)
			require.NoError(t, err)
			require.NoError(t, repo.Save(context.Background(), active))

			// Run Closer
			err = closer.CloseExpiredContests(context.Background(), repo, clock.FixedClock{Time: now})
			require.NoError(t, err)

			// Verify Expired
			updatedExpired, err := repo.FindByID(context.Background(), expired.ID)
			require.NoError(t, err)
			assert.Equal(t, contest.StatusClosed, updatedExpired.Status)

			// Verify Active
			updatedActive, err := repo.FindByID(context.Background(), active.ID)
			require.NoError(t, err)
			assert.Equal(t, contest.StatusOpen, updatedActive.Status)
		})

		t.Run("StartCloser_ClosesExpiredAndStops", func(t *testing.T) {
			repo := setup(t)
			circleID, userID := createPrerequisites(t, pool)

			now := time.Date(2026, 1, 5, 12, 0, 0, 0, time.UTC)

			// Create expired contest
			expiredClock := clock.FixedClock{Time: now.Add(-90 * time.Minute)}
			expired, err := contest.New(expiredClock, circleID, userID, "Expired?", []string{"Yes", "No"}, contest.Duration1Hour, 10)
			require.NoError(t, err)
			require.NoError(t, repo.Save(context.Background(), expired))

			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()

			// Run Closer in goroutine
			go closer.StartCloser(ctx, repo, clock.FixedClock{Time: now}, 50*time.Millisecond)

			// Wait enough time for the ticker to tick at least once
			time.Sleep(150 * time.Millisecond)
			cancel()

			// Verify
			updated, err := repo.FindByID(context.Background(), expired.ID)
			require.NoError(t, err)
			assert.Equal(t, contest.StatusClosed, updated.Status)
		})
	})
}
