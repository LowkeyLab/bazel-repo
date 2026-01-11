package closer

import (
	"context"
	"log"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
)

// ContestExpirer defines the interface for expiring contests with refunds.
type ContestExpirer interface {
	ExpireContest(ctx context.Context, contestID contest.ID) error
}

// StartCloser runs a background goroutine that periodically checks for:
// 1. Contests that need to be locked (voting closed).
// 2. Contests that have expired (auto-closed/cancelled).
// The goroutine stops when the context is canceled.
func StartCloser(ctx context.Context, repo repository.Repository, expirer ContestExpirer, clk clock.Clock, interval time.Duration) {
	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	log.Printf("Contest closer started with interval: %v", interval)

	for {
		select {
		case <-ctx.Done():
			log.Println("Contest closer stopping due to context cancellation")
			return
		case <-ticker.C:
			if err := LockContests(ctx, repo, clk); err != nil {
				log.Printf("Error locking contests: %v", err)
			}
			if err := ExpireContests(ctx, repo, expirer, clk); err != nil {
				log.Printf("Error expiring contests: %v", err)
			}
		}
	}
}

// LockContests finds contests that should be locked and updates their status.
func LockContests(ctx context.Context, repo repository.Repository, clk clock.Clock) error {
	toLock, err := repo.FindContestsToLock(ctx)
	if err != nil {
		return err
	}

	for _, c := range toLock {
		if c.IsLockedTime(clk) {
			if err := repo.UpdateStatus(ctx, c.ID, contest.StatusLocked); err != nil {
				log.Printf("Failed to lock contest %d: %v", c.ID, err)
				continue
			}
			log.Printf("Locked contest: %d (question: %s)", c.ID, c.Question)
		}
	}

	return nil
}

// ExpireContests finds contests that have expired and calls the expirer to close and refund them.
func ExpireContests(ctx context.Context, repo repository.Repository, expirer ContestExpirer, clk clock.Clock) error {
	toExpire, err := repo.FindContestsToExpire(ctx)
	if err != nil {
		return err
	}

	for _, c := range toExpire {
		if c.IsExpiredTime(clk) {
			if err := expirer.ExpireContest(ctx, c.ID); err != nil {
				log.Printf("Failed to expire (close & refund) contest %d: %v", c.ID, err)
				continue
			}
			log.Printf("Expired (closed & refunded) contest: %d (question: %s)", c.ID, c.Question)
		}
	}

	return nil
}
