package repository

import (
	"context"
	"fmt"
	"sync"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Memory is an in-memory implementation of the Repository interface.
type Memory struct {
	mu      sync.RWMutex
	circles map[circle.ID]*circle.Circle
	nextID  circle.ID
}

// NewMemory creates a new in-memory circle repository.
func NewMemory() *Memory {
	return &Memory{
		circles: make(map[circle.ID]*circle.Circle),
		nextID:  1,
	}
}

// Save persists a Circle and its members to memory.
func (r *Memory) Save(ctx context.Context, c *circle.Circle) error {
	if c == nil {
		return fmt.Errorf("circle cannot be nil")
	}

	r.mu.Lock()
	defer r.mu.Unlock()

	// If no ID, assign a new one
	if c.ID == 0 {
		c.ID = r.nextID
		r.nextID++
	}

	// Deep copy the circle to avoid external mutations
	circleCopy := &circle.Circle{
		ID:        c.ID,
		Name:      c.Name,
		CreatedAt: c.CreatedAt,
		Members:   make(map[user.ID]*circle.Member),
	}

	for userID, member := range c.Members {
		circleCopy.Members[userID] = &circle.Member{
			UserID: member.UserID,
			Clout:  member.Clout,
		}
	}

	r.circles[c.ID] = circleCopy
	return nil
}

// FindByID retrieves a Circle and its members by ID.
func (r *Memory) FindByID(ctx context.Context, id circle.ID) (*circle.Circle, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	c, exists := r.circles[id]
	if !exists {
		return nil, fmt.Errorf("circle not found with id: %d", id)
	}

	// Deep copy to avoid external mutations
	circleCopy := &circle.Circle{
		ID:        c.ID,
		Name:      c.Name,
		CreatedAt: c.CreatedAt,
		Members:   make(map[user.ID]*circle.Member),
	}

	for userID, member := range c.Members {
		circleCopy.Members[userID] = &circle.Member{
			UserID: member.UserID,
			Clout:  member.Clout,
		}
	}

	return circleCopy, nil
}

// AddMember persists a single member to an existing circle.
func (r *Memory) AddMember(ctx context.Context, circleID circle.ID, member *circle.Member) error {
	if member == nil {
		return fmt.Errorf("member cannot be nil")
	}

	r.mu.Lock()
	defer r.mu.Unlock()

	c, exists := r.circles[circleID]
	if !exists {
		return fmt.Errorf("circle not found with id: %d", circleID)
	}

	c.Members[member.UserID] = &circle.Member{
		UserID: member.UserID,
		Clout:  member.Clout,
	}

	return nil
}
