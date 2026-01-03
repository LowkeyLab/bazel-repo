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
	ID             ID
	CircleIDs      []circle.ID
	CreatorID      user.ID
	Question       string
	Options        map[int]*Option // Keyed by Option ID
	Predictions    []*Prediction
	Status         Status
	MinStake       int
	ResultOptionID *int // ID of the winning option
	CreatedAt      time.Time
	ExpiresAt      time.Time
}

const (
	minStakeDefault = 10
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
func New(circleIDs []circle.ID, creatorID user.ID, question string, options []string, expiresAt time.Time, minStake int) (*Contest, error) {
	if len(circleIDs) == 0 {
		return nil, errors.New("at least one circle is required")
	}

	uniqueCircles := make([]circle.ID, 0, len(circleIDs))
	circleSeen := make(map[circle.ID]struct{})
	for _, cid := range circleIDs {
		if cid == 0 {
			return nil, errors.New("circle id must be positive")
		}
		if _, exists := circleSeen[cid]; exists {
			continue
		}
		circleSeen[cid] = struct{}{}
		uniqueCircles = append(uniqueCircles, cid)
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
		ID:        0, // ID will be set by database
		CircleIDs: uniqueCircles,
		CreatorID: creatorID,
		Question:  question,
		Options:   optionMap,
		Status:    StatusOpen,
		MinStake:  normalizedMinStake,
		CreatedAt: time.Now(),
		ExpiresAt: expiresAt,
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

func normalizeMinStake(minStake int) (int, error) {
	if minStake == 0 {
		return minStakeDefault, nil
	}

	if _, ok := allowedMinStakes[minStake]; ok {
		return minStake, nil
	}

	return 0, fmt.Errorf("min stake must be one of 10, 100, or 1000")
}
