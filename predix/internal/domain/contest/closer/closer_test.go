package closer_test

import (
	"context"
	"testing"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/closer"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCloseExpiredContests_ClosesOnlyExpired(t *testing.T) {
	now := time.Date(2026, 1, 5, 12, 0, 0, 0, time.UTC)
	repo := repository.NewMemory()

	expiredClock := clock.FixedClock{Time: now.Add(-2 * time.Hour)}
	expired, err := contest.New(expiredClock, circle.ID(1), user.ID(1), "Expired?", []string{"Yes", "No"}, contest.Duration1Hour, 10)
	require.NoError(t, err)
	require.NoError(t, repo.Save(context.Background(), expired))

	activeClock := clock.FixedClock{Time: now}
	active, err := contest.New(activeClock, circle.ID(2), user.ID(1), "Active?", []string{"A", "B"}, contest.Duration1Hour, 10)
	require.NoError(t, err)
	require.NoError(t, repo.Save(context.Background(), active))

	err = closer.CloseExpiredContests(context.Background(), repo, clock.FixedClock{Time: now})
	require.NoError(t, err)

	updatedExpired, err := repo.FindByID(context.Background(), expired.ID)
	require.NoError(t, err)
	assert.Equal(t, contest.StatusClosed, updatedExpired.Status)

	updatedActive, err := repo.FindByID(context.Background(), active.ID)
	require.NoError(t, err)
	assert.Equal(t, contest.StatusOpen, updatedActive.Status)
}

func TestStartCloser_ClosesExpiredAndStops(t *testing.T) {
	now := time.Date(2026, 1, 5, 12, 0, 0, 0, time.UTC)
	repo := repository.NewMemory()

	expiredClock := clock.FixedClock{Time: now.Add(-90 * time.Minute)}
	expired, err := contest.New(expiredClock, circle.ID(3), user.ID(1), "Expired?", []string{"Yes", "No"}, contest.Duration1Hour, 10)
	require.NoError(t, err)
	require.NoError(t, repo.Save(context.Background(), expired))

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	go closer.StartCloser(ctx, repo, clock.FixedClock{Time: now}, 5*time.Millisecond)

	time.Sleep(20 * time.Millisecond)
	cancel()
	time.Sleep(5 * time.Millisecond)

	updated, err := repo.FindByID(context.Background(), expired.ID)
	require.NoError(t, err)
	assert.Equal(t, contest.StatusClosed, updated.Status)
}
