package closer

import (
	"context"
	"log"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
)

// StartCloser runs a background goroutine that periodically checks for expired contests
// and closes them by updating their status to CLOSED. The goroutine stops when the context is canceled.
func StartCloser(ctx context.Context, repo repository.Repository, clk clock.Clock, interval time.Duration) {
	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	log.Printf("Contest closer started with interval: %v", interval)

	for {
		select {
		case <-ctx.Done():
			log.Println("Contest closer stopping due to context cancellation")
			return
		case <-ticker.C:
			if err := CloseExpiredContests(ctx, repo, clk); err != nil {
				log.Printf("Error closing expired contests: %v", err)
			}
		}
	}
}

// CloseExpiredContests finds and closes all expired contests.
func CloseExpiredContests(ctx context.Context, repo repository.Repository, clk clock.Clock) error {
	expiredContests, err := repo.FindExpiredContests(ctx)
	if err != nil {
		return err
	}

	for _, c := range expiredContests {
		if c.IsClosed(clk) {
			if err := repo.UpdateStatus(ctx, c.ID, contest.StatusClosed); err != nil {
				log.Printf("Failed to close contest %d: %v", c.ID, err)
				continue
			}
			log.Printf("Closed expired contest: %d (question: %s)", c.ID, c.Question)
		}
	}

	return nil
}
