package closer

import (
	"context"
	"log/slog"
	"time"
)

// ContestManager defines the interface for managing contest state transitions.
type ContestManager interface {
	ProcessContestLocks(ctx context.Context) error
	ProcessContestExpirations(ctx context.Context) error
}

// StartCloser runs a background goroutine that periodically checks for:
// 1. Contests that need to be locked (voting closed).
// 2. Contests that have expired (auto-closed/cancelled).
// The goroutine stops when the context is canceled.
func StartCloser(ctx context.Context, manager ContestManager, interval time.Duration) {
	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	slog.Info("Contest closer started", "interval", interval)

	for {
		select {
		case <-ctx.Done():
			slog.Info("Contest closer stopping due to context cancellation")
			return
		case <-ticker.C:
			if err := manager.ProcessContestLocks(ctx); err != nil {
				slog.Error("Error processing contest locks", "error", err)
			}
			if err := manager.ProcessContestExpirations(ctx); err != nil {
				slog.Error("Error processing contest expirations", "error", err)
			}
		}
	}
}
