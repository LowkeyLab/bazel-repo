package contest_test

import (
	"testing"
	"time"

	clockpkg "github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCalculateWinnerPayouts(t *testing.T) {
	circleID := circle.ID(1)
	creatorID := user.ID(1)
	clk := clockpkg.FixedClock{Time: time.Now()}
	c, err := contest.New(clk, circleID, creatorID, "Q?", []string{"Win", "Lose"}, contest.Duration1Hour, 10)
	require.NoError(t, err)

	winnerID := user.ID(2)
	loserID := user.ID(3)

	// User 2 bets 100 on Option 1 (Winner)
	err = c.Predict(winnerID, 1, 100)
	require.NoError(t, err)

	// User 3 bets 100 on Option 2 (Loser)
	err = c.Predict(loserID, 2, 100)
	require.NoError(t, err)

	// Resolve contest with Option 1 winning
	err = c.Resolve(1)
	require.NoError(t, err)

	// Calculate payouts
	payouts, err := c.CalculateWinnerPayouts()
	require.NoError(t, err)

	// Expected:
	// Total Pot = 200
	// Winning Stake = 100
	// Losing Stake = 100
	// Consumption Rate = 0.10 (Default)
	// Burn = 100 * 0.10 = 10
	// Distributable from Losers = 100 - 10 = 90
	// Winner Payout = Original Stake (100) + Share (1.0) * Distributable (90) = 190

	expectedPayout := 190
	assert.Equal(t, expectedPayout, payouts[winnerID], "Winner should receive original stake + 90% of losing stake")
	assert.Equal(t, 0, payouts[loserID], "Loser should receive 0")
}

func TestCalculateWinnerPayouts_MultipleWinners(t *testing.T) {
	circleID := circle.ID(1)
	creatorID := user.ID(1)
	clk := clockpkg.FixedClock{Time: time.Now()}
	c, err := contest.New(clk, circleID, creatorID, "Q?", []string{"Win", "Lose"}, contest.Duration1Hour, 10)
	require.NoError(t, err)

	winner1ID := user.ID(2) // Bets 100
	winner2ID := user.ID(3) // Bets 300
	loserID := user.ID(4)   // Bets 400

	require.NoError(t, c.Predict(winner1ID, 1, 100))
	require.NoError(t, c.Predict(winner2ID, 1, 300))
	require.NoError(t, c.Predict(loserID, 2, 400))

	require.NoError(t, c.Resolve(1))

	payouts, err := c.CalculateWinnerPayouts()
	require.NoError(t, err)

	// Winning Stake Total = 400
	// Losing Stake Total = 400
	// Burn = 400 * 0.10 = 40
	// Distributable = 360

	// Winner 1 Share = 100/400 = 0.25
	// Winner 1 Payout = 100 + 0.25 * 360 = 100 + 90 = 190

	// Winner 2 Share = 300/400 = 0.75
	// Winner 2 Payout = 300 + 0.75 * 360 = 300 + 270 = 570

	assert.Equal(t, 190, payouts[winner1ID])
	assert.Equal(t, 570, payouts[winner2ID])
}
