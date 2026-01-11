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

	// Create User
	u, err := q.CreateUser(ctx, db.CreateUserParams{
		Username:     "testuser",
		PasswordHash: "hash",
		Role:         db.UserRoleMember,
	})
	require.NoError(t, err)

	// Create Circle
	c, err := q.CreateCircle(ctx, db.CreateCircleParams{
		Name:      "Test Circle",
		CreatorID: u.ID,
		CreatedAt: pgtype.Timestamp{Time: time.Now(), Valid: true},
	})
	require.NoError(t, err)

	return circle.ID(c.ID), user.ID(u.ID)
}

// MockExpirer implements ContestExpirer for testing
type MockExpirer struct {
	expiredContests []contest.ID
	repo            repository.Repository
}

func (m *MockExpirer) ExpireContest(ctx context.Context, contestID contest.ID) error {
	m.expiredContests = append(m.expiredContests, contestID)
	// Simulate what the service does: expire the contest
	// In real service, it also refunds, but here we just check status update via this call
	return m.repo.UpdateStatus(ctx, contestID, contest.StatusExpired)
}

func TestCloser(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, pool *pgxpool.Pool) {
		setup := func(t *testing.T) (*repository.Postgres, *MockExpirer) {
			testutil.ResetTables(t, pool)
			repo := repository.NewPostgres(pool)
			return repo, &MockExpirer{repo: repo}
		}

		t.Run("LockContests", func(t *testing.T) {
			repo, _ := setup(t)
			circleID, userID := createPrerequisites(t, pool)

			now := time.Date(2026, 1, 5, 12, 0, 0, 0, time.UTC)

			// Create contest that should be locked (passed 1 hour duration)
			toLockClock := clock.FixedClock{Time: now.Add(-2 * time.Hour)}
			toLock, err := contest.New(toLockClock, circleID, userID, "To Lock?", []string{"Yes", "No"}, contest.Duration1Hour, 10)
			require.NoError(t, err)
			require.NoError(t, repo.Save(context.Background(), toLock))

			// Create active contest (created now)
			activeClock := clock.FixedClock{Time: now}
			active, err := contest.New(activeClock, circleID, userID, "Active?", []string{"A", "B"}, contest.Duration1Hour, 10)
			require.NoError(t, err)
			require.NoError(t, repo.Save(context.Background(), active))

			// Run LockContests
			err = closer.LockContests(context.Background(), repo, clock.FixedClock{Time: now})
			require.NoError(t, err)

			// Verify Locked
			updatedToLock, err := repo.FindByID(context.Background(), toLock.ID)
			require.NoError(t, err)
			assert.Equal(t, contest.StatusLocked, updatedToLock.Status)

			// Verify Active
			updatedActive, err := repo.FindByID(context.Background(), active.ID)
			require.NoError(t, err)
			assert.Equal(t, contest.StatusOpen, updatedActive.Status)
		})

		t.Run("ExpireContests", func(t *testing.T) {
			repo, expirer := setup(t)
			circleID, userID := createPrerequisites(t, pool)

			now := time.Date(2026, 1, 5, 12, 0, 0, 0, time.UTC)

			// Create contest that should be expired (passed 1 hour duration + 7 days expiration)
			expiredClock := clock.FixedClock{Time: now.Add(-8 * 24 * time.Hour)}
			expired, err := contest.New(expiredClock, circleID, userID, "Expired?", []string{"Yes", "No"}, contest.Duration1Hour, 10)
			require.NoError(t, err)
			expired.Status = contest.StatusLocked
			require.NoError(t, repo.Save(context.Background(), expired))

			// Create contest that is just locked but not expired
			justLockedClock := clock.FixedClock{Time: now.Add(-2 * time.Hour)}
			justLocked, err := contest.New(justLockedClock, circleID, userID, "Just Locked?", []string{"A", "B"}, contest.Duration1Hour, 10)
			require.NoError(t, err)
			justLocked.Status = contest.StatusLocked
			require.NoError(t, repo.Save(context.Background(), justLocked))

			// Run ExpireContests
			err = closer.ExpireContests(context.Background(), repo, expirer, clock.FixedClock{Time: now})
			require.NoError(t, err)

			// Verify Expired via Mock
			assert.Len(t, expirer.expiredContests, 1)
			assert.Equal(t, expired.ID, expirer.expiredContests[0])

			// Verify Status (updated by Mock)
			updatedExpired, err := repo.FindByID(context.Background(), expired.ID)
			require.NoError(t, err)
			assert.Equal(t, contest.StatusExpired, updatedExpired.Status)

			// Verify Just Locked is still Locked
			updatedJustLocked, err := repo.FindByID(context.Background(), justLocked.ID)
			require.NoError(t, err)
			assert.Equal(t, contest.StatusLocked, updatedJustLocked.Status)
		})

		t.Run("StartCloser_RunsBoth", func(t *testing.T) {
			repo, expirer := setup(t)
			circleID, userID := createPrerequisites(t, pool)

			now := time.Date(2026, 1, 5, 12, 0, 0, 0, time.UTC)

			// 1. Contest to Lock
			toLockClock := clock.FixedClock{Time: now.Add(-2 * time.Hour)}
			toLock, err := contest.New(toLockClock, circleID, userID, "To Lock", []string{"Y", "N"}, contest.Duration1Hour, 10)
			require.NoError(t, err)
			require.NoError(t, repo.Save(context.Background(), toLock))

			// 2. Contest to Expire
			expiredClock := clock.FixedClock{Time: now.Add(-8 * 24 * time.Hour)}
			toExpire, err := contest.New(expiredClock, circleID, userID, "To Expire", []string{"Y", "N"}, contest.Duration1Hour, 10)
			require.NoError(t, err)
			toExpire.Status = contest.StatusLocked
			require.NoError(t, repo.Save(context.Background(), toExpire))

			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()

			// Run Closer
			go closer.StartCloser(ctx, repo, expirer, clock.FixedClock{Time: now}, 50*time.Millisecond)

			time.Sleep(150 * time.Millisecond)
			cancel()

			// Verify Lock
			updatedToLock, err := repo.FindByID(context.Background(), toLock.ID)
			require.NoError(t, err)
			assert.Equal(t, contest.StatusLocked, updatedToLock.Status)

			// Verify Expire call
			assert.Contains(t, expirer.expiredContests, toExpire.ID)
		})
	})
}
