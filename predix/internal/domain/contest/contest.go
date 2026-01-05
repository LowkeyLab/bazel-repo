package contest

import (
	"errors"
	"fmt"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// ID represents the unique identifier for a Contest.
type ID int32

// Status represents the state of a contest.
type Status string

const (
	StatusOpen     Status = "OPEN"
	StatusClosed   Status = "CLOSED"
	StatusResolved Status = "RESOLVED"
)

// Contest represents an event users can predict on.
type Contest struct {
	ID              ID
	CircleID        circle.ID
	CreatorID       user.ID
	Question        string
	Options         map[int]*Option // Keyed by Option ID
	Predictions     []*Prediction
	Status          Status
	MinStake        int
	ResultOptionID  *int    // ID of the winning option
	ConsumptionRate float64 // Rate at which clout is consumed (e.g., 0.10 for 10%)
	CreatedAt       time.Time
	ExpiresAt       time.Time
}

const (
	minStakeDefault        = 10
	consumptionRateDefault = 0.10 // 10% consumption rate
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
func New(circleID circle.ID, creatorID user.ID, question string, options []string, expiresAt time.Time, minStake int) (*Contest, error) {
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

	return &Contest{
		ID:              0, // ID will be set by database
		CircleID:        circleID,
		CreatorID:       creatorID,
		Question:        question,
		Options:         optionMap,
		Status:          StatusOpen,
		MinStake:        normalizedMinStake,
		ConsumptionRate: consumptionRateDefault,
		CreatedAt:       time.Now(),
		ExpiresAt:       expiresAt,
	}, nil
}

// Predict adds a prediction to the contest.
func (c *Contest) Predict(userID user.ID, optionID int, clout int) error {
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

	prediction := &Prediction{
		UserID:    userID,
		OptionID:  optionID,
		Clout:     clout,
		Timestamp: time.Now(),
	}
	c.Predictions = append(c.Predictions, prediction)
	return nil
}

// Resolve marks the contest as resolved and determines the winner.
func (c *Contest) Resolve(winningOptionID int) error {
	if c.Status == StatusResolved {
		return errors.New("contest is already resolved")
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

// CalculateConsumedClout returns the amount of clout consumed at the current rate.
func (c *Contest) CalculateConsumedClout() int {
	pot := c.CalculatePot()
	return int(float64(pot) * c.ConsumptionRate)
}

// CalculateRemainingPot returns the remaining pot after consumption (90% of total pot).
func (c *Contest) CalculateRemainingPot() int {
	pot := c.CalculatePot()
	return pot - c.CalculateConsumedClout()
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
