package repository

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
)

// Memory is an in-memory implementation of the Repository interface.
type Memory struct {
	mu       sync.RWMutex
	contests map[contest.ID]*contest.Contest
	nextID   contest.ID
}

// NewMemory creates a new in-memory contest repository.
func NewMemory() *Memory {
	return &Memory{
		contests: make(map[contest.ID]*contest.Contest),
		nextID:   1,
	}
}

// Save persists a Contest with its options and predictions to memory.
func (r *Memory) Save(ctx context.Context, c *contest.Contest) error {
	if c == nil {
		return fmt.Errorf("contest cannot be nil")
	}

	r.mu.Lock()
	defer r.mu.Unlock()

	// If no ID, assign a new one
	if c.ID == 0 {
		c.ID = r.nextID
		r.nextID++
	}

	r.contests[c.ID] = r.deepCopy(c)
	return nil
}

// FindByID retrieves a Contest by ID.
func (r *Memory) FindByID(ctx context.Context, id contest.ID) (*contest.Contest, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	c, exists := r.contests[id]
	if !exists {
		return nil, fmt.Errorf("contest not found with id: %d", id)
	}

	return r.deepCopy(c), nil
}

// FindByCircleID retrieves all contests associated with a circle.
func (r *Memory) FindByCircleID(ctx context.Context, circleID circle.ID) ([]*contest.Contest, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	var result []*contest.Contest

	for _, c := range r.contests {
		if c.CircleID == circleID {
			result = append(result, r.deepCopy(c))
		}
	}

	return result, nil
}

// FindContestsToLock retrieves all contests that are OPEN and have passed their locked_at time.
func (r *Memory) FindContestsToLock(ctx context.Context) ([]*contest.Contest, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	var result []*contest.Contest
	now := time.Now()

	for _, c := range r.contests {
		if c.Status == contest.StatusOpen && !now.Before(c.LockedAt) {
			result = append(result, r.deepCopy(c))
		}
	}

	return result, nil
}

// FindContestsToExpire retrieves all contests that are OPEN or LOCKED and have passed their expires_at time.
func (r *Memory) FindContestsToExpire(ctx context.Context) ([]*contest.Contest, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	var result []*contest.Contest
	now := time.Now()

	for _, c := range r.contests {
		if (c.Status == contest.StatusOpen || c.Status == contest.StatusLocked) && !now.Before(c.ExpiresAt) {
			result = append(result, r.deepCopy(c))
		}
	}

	return result, nil
}

// UpdateStatus updates the status of a contest.
func (r *Memory) UpdateStatus(ctx context.Context, id contest.ID, status contest.Status) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	c, exists := r.contests[id]
	if !exists {
		return fmt.Errorf("contest not found with id: %d", id)
	}

	c.Status = status
	return nil
}

func (r *Memory) deepCopy(c *contest.Contest) *contest.Contest {
	minStake := c.MinStake
	if minStake == 0 {
		minStake = defaultMinStake
	}

	contestCopy := &contest.Contest{
		ID:             c.ID,
		CircleID:       c.CircleID,
		CreatorID:      c.CreatorID,
		Question:       c.Question,
		Status:         c.Status,
		MinStake:       minStake,
		CreatedAt:      c.CreatedAt,
		LockedAt:       c.LockedAt,
		ExpiresAt:      c.ExpiresAt,
		Duration:       c.Duration,
		Options:        make(map[int]*contest.Option),
		Predictions:    make([]*contest.Prediction, len(c.Predictions)),
		ResultOptionID: c.ResultOptionID,
		HouseRake:      c.HouseRake,
	}

	for id, option := range c.Options {
		contestCopy.Options[id] = &contest.Option{
			ID:   option.ID,
			Text: option.Text,
		}
	}

	for i, prediction := range c.Predictions {
		contestCopy.Predictions[i] = &contest.Prediction{
			UserID:    prediction.UserID,
			OptionID:  prediction.OptionID,
			Clout:     prediction.Clout,
			Timestamp: prediction.Timestamp,
		}
	}

	return contestCopy
}
