package contest

import (
	"errors"
	"time"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// ID represents the unique identifier for a Contest.
type ID uuid.UUID

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
	CircleID       circle.ID
	CreatorID      user.ID
	Question       string
	Options        map[int]*Option // Keyed by Option ID
	Predictions    []*Prediction
	Status         Status
	ResultOptionID *int // ID of the winning option
	CreatedAt      time.Time
	ExpiresAt      time.Time
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
func New(circleID circle.ID, creatorID user.ID, question string, options []string, expiresAt time.Time) (*Contest, error) {
	if len(options) < 2 {
		return nil, errors.New("at least two options are required")
	}

	id, err := uuid.NewRandom()
	if err != nil {
		return nil, err
	}

	optionMap := make(map[int]*Option)
	for i, text := range options {
		optID := i + 1
		optionMap[optID] = &Option{ID: optID, Text: text}
	}

	return &Contest{
		ID:        ID(id),
		CircleID:  circleID,
		CreatorID: creatorID,
		Question:  question,
		Options:   optionMap,
		Status:    StatusOpen,
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

func (id ID) String() string {
	return uuid.UUID(id).String()
}
