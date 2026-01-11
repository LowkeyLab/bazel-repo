package closer_test

import (
	"context"
	"testing"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/closer"
	"github.com/stretchr/testify/assert"
)

// MockManager implements ContestManager for testing
type MockManager struct {
	locksProcessed       int
	expirationsProcessed int
}

func (m *MockManager) ProcessContestLocks(ctx context.Context) error {
	m.locksProcessed++
	return nil
}

func (m *MockManager) ProcessContestExpirations(ctx context.Context) error {
	m.expirationsProcessed++
	return nil
}

func TestStartCloser(t *testing.T) {
	manager := &MockManager{}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Use a very short interval for testing
	interval := 10 * time.Millisecond

	// Start closer in background
	go closer.StartCloser(ctx, manager, interval)

	// Wait for a few ticks
	time.Sleep(50 * time.Millisecond)
	cancel()

	// Allow some time for goroutine to exit (optional but good practice)
	time.Sleep(10 * time.Millisecond)

	assert.Greater(t, manager.locksProcessed, 0, "ProcessContestLocks should have been called")
	assert.Greater(t, manager.expirationsProcessed, 0, "ProcessContestExpirations should have been called")
}
