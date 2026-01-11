package contest

import (
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Duration constants for contest expiration.
const (
	Duration1Hour = "1h"
	Duration1Day  = "1d"
	Duration1Week = "1w"
)

// DefaultExpirationAfterClose is the time after locking when a contest expires (refunds).
const DefaultExpirationAfterClose = 7 * 24 * time.Hour

// ValidDurations returns all accepted duration values.
func ValidDurations() []string {
	return []string{Duration1Hour, Duration1Day, Duration1Week}
}

// parseDuration converts a duration string to a time.Duration.
func parseDuration(d string) (time.Duration, error) {
	switch d {
	case Duration1Hour:
		return time.Hour, nil
	case Duration1Day:
		return 24 * time.Hour, nil
	case Duration1Week:
		return 7 * 24 * time.Hour, nil
	default:
		return 0, fmt.Errorf("invalid duration: must be one of: %s", strings.Join(ValidDurations(), ", "))
	}
}

// ID represents the unique identifier for a Contest.
type ID int32

// Status represents the state of a contest.
type Status string

const (
	StatusOpen     Status = "OPEN"
	StatusLocked   Status = "LOCKED"
	StatusClosed   Status = "CLOSED"
	StatusResolved Status = "RESOLVED"
)

// Contest represents an event users can predict on.
type Contest struct {
	ID             ID
	CircleID       circle.ID
	CreatorID      user.ID
	Question       string
	Options        map[int]*Option // Keyed by Option ID
	Predictions    []*Prediction
	Status         Status
	MinStake       int
	ResultOptionID *int    // ID of the winning option
	HouseRake      float64 // Rate at which clout is taken as house rake (e.g., 0.10 for 10%)
	CreatedAt      time.Time
	ClosesAt       time.Time // Time when predictions are locked
	ExpiresAt      time.Time // Time when contest is auto-closed and refunded
	Duration       string    // "1h", "1d", or "1w"
}

const (
	minStakeDefault  = 10
	houseRakeDefault = 0.10 // 10% house rake
)

var allowedMinStakes = map[int]struct{}{
	10:   {},
	100:  {},
	1000: {},
}

// Option represents a choice in the contest.
type Option struct {
	ID   int
	Text string
}

// Prediction represents a wager by a user on a specific option.
type Prediction struct {
	UserID    user.ID
	OptionID  int
	Clout     int
	Timestamp time.Time
}

// New creates a new Contest.
func New(clk clock.Clock, circleID circle.ID, creatorID user.ID, question string, options []string, duration string, minStake int) (*Contest, error) {
	if circleID == 0 {
		return nil, errors.New("circle id must be positive")
	}

	if question == "" {
		return nil, errors.New("contest question cannot be empty")
	}

	if len(options) < 2 {
		return nil, errors.New("at least two options are required")
	}

	normalizedMinStake, err := normalizeMinStake(minStake)
	if err != nil {
		return nil, err
	}

	// Validate and parse duration
	durationValue, err := parseDuration(duration)
	if err != nil {
		return nil, err
	}

	// Validate all options are non-empty
	for i, opt := range options {
		if opt == "" {
			return nil, errors.New("option text cannot be empty")
		}
		// Check for duplicate options
		for j := i + 1; j < len(options); j++ {
			if opt == options[j] {
				return nil, errors.New("duplicate options are not allowed")
			}
		}
	}

	optionMap := make(map[int]*Option)
	for i, text := range options {
		optID := i + 1
		optionMap[optID] = &Option{ID: optID, Text: text}
	}

	now := clk.Now()
	closesAt := now.Add(durationValue)

	return &Contest{
		ID:        0, // ID will be set by database
		CircleID:  circleID,
		CreatorID: creatorID,
		Question:  question,
		Options:   optionMap,
		Status:    StatusOpen,
		MinStake:  normalizedMinStake,
		HouseRake: houseRakeDefault,
		CreatedAt: now,
		ClosesAt:  closesAt,
		ExpiresAt: closesAt.Add(DefaultExpirationAfterClose),
		Duration:  duration,
	}, nil
}

// Predict adds a prediction to the contest.
func (c *Contest) Predict(userID user.ID, optionID int, clout int) error {
	if c.Status == StatusLocked {
		return errors.New("contest is locked for predictions")
	}
	if c.Status != StatusOpen {
		return errors.New("contest is not open for predictions")
	}
	if _, ok := c.Options[optionID]; !ok {
		return errors.New("invalid option")
	}
	if clout <= 0 {
		return errors.New("prediction clout must be positive")
	}
	if clout < c.MinStake {
		return fmt.Errorf("prediction clout must be at least %d", c.MinStake)
	}

	now := time.Now()
	for _, prediction := range c.Predictions {
		if prediction.UserID == userID && prediction.OptionID == optionID {
			prediction.Clout = clout
			prediction.Timestamp = now
			return nil
		}
	}

	prediction := &Prediction{
		UserID:    userID,
		OptionID:  optionID,
		Clout:     clout,
		Timestamp: now,
	}
	c.Predictions = append(c.Predictions, prediction)
	return nil
}

// IsLockedTime checks if the contest should be locked based on the current time.
// Returns true if the contest status is OPEN and the closes_at time has passed.
func (c *Contest) IsLockedTime(clk clock.Clock) bool {
	if c.Status != StatusOpen {
		return false
	}
	return !clk.Now().Before(c.ClosesAt)
}

// IsExpiredTime checks if the contest should be expired based on the current time.
// Returns true if the contest status is OPEN or LOCKED and the expires_at time has passed.
func (c *Contest) IsExpiredTime(clk clock.Clock) bool {
	if c.Status != StatusOpen && c.Status != StatusLocked {
		return false
	}
	return !clk.Now().Before(c.ExpiresAt)
}

// Lock transitions the contest from Open to Locked, preventing new predictions.
func (c *Contest) Lock() error {
	if c.Status != StatusOpen {
		return errors.New("only open contests can be locked")
	}

	c.Status = StatusLocked
	return nil
}

// Close transitions the contest to Closed without resolving it.
// This refunds all staked clouts to the predictors.
// Only open or locked contests can be closed.
func (c *Contest) Close() error {
	if c.Status == StatusResolved {
		return errors.New("cannot close a resolved contest")
	}
	if c.Status == StatusClosed {
		return errors.New("contest is already closed")
	}
	if c.Status != StatusLocked && c.Status != StatusOpen {
		return errors.New("contest is not in a state that can be closed")
	}

	c.Status = StatusClosed
	return nil
}

// Resolve marks the contest as resolved and determines the winner.
func (c *Contest) Resolve(winningOptionID int) error {
	if c.Status == StatusResolved {
		return errors.New("contest is already resolved")
	}
	if c.Status != StatusLocked && c.Status != StatusOpen {
		return errors.New("contest is not in a state that can be resolved")
	}
	if _, ok := c.Options[winningOptionID]; !ok {
		return errors.New("invalid winning option")
	}

	c.ResultOptionID = &winningOptionID
	c.Status = StatusResolved
	return nil
}

// CalculatePot returns the sum of all predictions' clout.
func (c *Contest) CalculatePot() int {
	total := 0
	for _, pred := range c.Predictions {
		total += pred.Clout
	}
	return total
}

// CalculateHouseRakeClout returns the amount of clout taken as house rake from losing stakes.
// Returns 0 if the contest is not resolved.
func (c *Contest) CalculateHouseRakeClout() int {
	if c.ResultOptionID == nil {
		return 0
	}

	totalPot := 0
	winningStake := 0
	for _, pred := range c.Predictions {
		totalPot += pred.Clout
		if pred.OptionID == *c.ResultOptionID {
			winningStake += pred.Clout
		}
	}

	losingStake := totalPot - winningStake
	return int(float64(losingStake) * c.HouseRake)
}

// CalculateRemainingPot returns the remaining pot after consumption.
func (c *Contest) CalculateRemainingPot() int {
	pot := c.CalculatePot()
	return pot - c.CalculateHouseRakeClout()
}

// PayoutRecord represents the payout details for a single winner.
type PayoutRecord struct {
	UserID        user.ID
	OriginalStake int
	ShareOfPot    int
	TotalPayout   int
}

// CalculateWinnerPayouts calculates and returns the clout distribution to winners.
// It validates that the contest is resolved and returns a map of user.ID to total payout.
// Returns an error if the contest hasn't been resolved or has no winning option.
func (c *Contest) CalculateWinnerPayouts() (map[user.ID]int, error) {
	if c.ResultOptionID == nil {
		return nil, errors.New("contest has not been resolved yet")
	}

	payouts := make(map[user.ID]int)

	// Find all predictions that match the winning option
	var winningPredictions []*Prediction
	var totalWinningClout int
	var totalPot int

	for i := range c.Predictions {
		totalPot += c.Predictions[i].Clout
		if c.Predictions[i].OptionID == *c.ResultOptionID {
			winningPredictions = append(winningPredictions, c.Predictions[i])
			totalWinningClout += c.Predictions[i].Clout
		}
	}

	if len(winningPredictions) == 0 {
		return payouts, nil
	}

	losingClout := totalPot - totalWinningClout
	distributableLosingClout := int(float64(losingClout) * (1.0 - c.HouseRake))

	for _, pred := range winningPredictions {
		// Return original stake
		payout := pred.Clout
		// Add proportional share of remaining pot (after consumption fee)
		if totalWinningClout > 0 {
			payout += (pred.Clout * distributableLosingClout) / totalWinningClout
		}
		payouts[pred.UserID] += payout
	}

	return payouts, nil
}

// CalculatePayoutBreakdown returns detailed payout records for all winners and losers.
// Returns two slices: winning PayoutRecords and losing PayoutRecords.
// Returns an error if the contest hasn't been resolved or has no winning option.
func (c *Contest) CalculatePayoutBreakdown() ([]*PayoutRecord, []*PayoutRecord, error) {
	if c.ResultOptionID == nil {
		return nil, nil, errors.New("contest has not been resolved yet")
	}

	var winners []*PayoutRecord
	var losers []*PayoutRecord

	// Find all predictions that match the winning option
	var winningPredictions []*Prediction
	var losingPredictions []*Prediction
	var totalWinningClout int
	var totalPot int

	for i := range c.Predictions {
		totalPot += c.Predictions[i].Clout
		if c.Predictions[i].OptionID == *c.ResultOptionID {
			winningPredictions = append(winningPredictions, c.Predictions[i])
			totalWinningClout += c.Predictions[i].Clout
		} else {
			losingPredictions = append(losingPredictions, c.Predictions[i])
		}
	}

	losingClout := totalPot - totalWinningClout
	distributableLosingClout := int(float64(losingClout) * (1.0 - c.HouseRake))

	// Create payout records for each winner
	for _, pred := range winningPredictions {
		share := 0
		if totalWinningClout > 0 {
			share = (pred.Clout * distributableLosingClout) / totalWinningClout
		}

		record := &PayoutRecord{
			UserID:        pred.UserID,
			OriginalStake: pred.Clout,
			ShareOfPot:    share,
			TotalPayout:   pred.Clout + share,
		}
		winners = append(winners, record)
	}

	// Create payout records for each loser
	for _, pred := range losingPredictions {
		record := &PayoutRecord{
			UserID:        pred.UserID,
			OriginalStake: pred.Clout,
			ShareOfPot:    0,
			TotalPayout:   0,
		}
		losers = append(losers, record)
	}

	return winners, losers, nil
}

func normalizeMinStake(minStake int) (int, error) {
	if minStake == 0 {
		return minStakeDefault, nil
	}

	if _, ok := allowedMinStakes[minStake]; ok {
		return minStake, nil
	}

	return 0, fmt.Errorf("min stake must be one of 10, 100, or 1000")
}
