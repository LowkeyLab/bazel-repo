package prediction

import (
	"errors"
	"time"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/domain/user"
)

// ID represents the unique identifier for a Prediction.
type ID uuid.UUID

// Status represents the state of a prediction.
type Status string

const (
	StatusOpen     Status = "OPEN"
	StatusClosed   Status = "CLOSED"
	StatusResolved Status = "RESOLVED"
)

// Prediction represents an event users can bet on.
type Prediction struct {
	ID             ID
	CircleID       circle.ID
	CreatorID      user.ID
	Question       string
	Options        map[string]*Option // Keyed by Option ID
	Bets           []*Bet
	Status         Status
	ResultOptionID *string // ID of the winning option
	CreatedAt      time.Time
	ExpiresAt      time.Time
}

// Option represents a choice in the prediction.
type Option struct {
	ID   string
	Text string
}

// Bet represents a wager by a user on a specific option.
type Bet struct {
	ID        string
	UserID    user.ID
	OptionID  string
	Amount    int
	Timestamp time.Time
}

// New creates a new Prediction.
func New(circleID circle.ID, creatorID user.ID, question string, options []string, expiresAt time.Time) (*Prediction, error) {
	if len(options) < 2 {
		return nil, errors.New("at least two options are required")
	}

	id, err := uuid.NewRandom()
	if err != nil {
		return nil, err
	}

	optionMap := make(map[string]*Option)
	for _, text := range options {
		optID := uuid.New().String()
		optionMap[optID] = &Option{ID: optID, Text: text}
	}

	return &Prediction{
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

// PlaceBet adds a bet to the prediction.
func (p *Prediction) PlaceBet(userID user.ID, optionID string, amount int) error {
	if p.Status != StatusOpen {
		return errors.New("prediction is not open for betting")
	}
	if _, ok := p.Options[optionID]; !ok {
		return errors.New("invalid option")
	}
	if amount <= 0 {
		return errors.New("bet amount must be positive")
	}

	bet := &Bet{
		ID:        uuid.New().String(),
		UserID:    userID,
		OptionID:  optionID,
		Amount:    amount,
		Timestamp: time.Now(),
	}
	p.Bets = append(p.Bets, bet)
	return nil
}

// Resolve marks the prediction as resolved and determines the winner.
func (p *Prediction) Resolve(winningOptionID string) error {
	if p.Status == StatusResolved {
		return errors.New("prediction is already resolved")
	}
	if _, ok := p.Options[winningOptionID]; !ok {
		return errors.New("invalid winning option")
	}

	p.ResultOptionID = &winningOptionID
	p.Status = StatusResolved
	return nil
}

func (id ID) String() string {
	return uuid.UUID(id).String()
}
