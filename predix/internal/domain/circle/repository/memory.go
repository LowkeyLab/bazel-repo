package repository

import (
	"context"
	"fmt"
	"sync"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Memory is an in-memory implementation of circle repository interfaces.
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
		CreatorID: c.CreatorID,
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
		CreatorID: c.CreatorID,
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

// FindByUserID retrieves all circles for a given user.
func (r *Memory) FindByUserID(ctx context.Context, userID int32) ([]*circle.Circle, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	var userCircles []*circle.Circle

	for _, c := range r.circles {
		// Check if user is a member of this circle
		if _, isMember := c.Members[user.ID(userID)]; isMember {
			// Deep copy to avoid external mutations
			circleCopy := &circle.Circle{
				ID:        c.ID,
				Name:      c.Name,
				CreatorID: c.CreatorID,
				CreatedAt: c.CreatedAt,
				Members:   make(map[user.ID]*circle.Member),
			}

			for memberUserID, member := range c.Members {
				circleCopy.Members[memberUserID] = &circle.Member{
					UserID: member.UserID,
					Clout:  member.Clout,
				}
			}

			userCircles = append(userCircles, circleCopy)
		}
	}

	return userCircles, nil
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

// Delete removes a circle and its members from memory.
func (r *Memory) Delete(ctx context.Context, circleID circle.ID) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if _, exists := r.circles[circleID]; !exists {
		return fmt.Errorf("circle not found with id: %d", circleID)
	}

	delete(r.circles, circleID)
	return nil
}

// UpdateMemberClout updates a member's clout balance in a circle.
func (r *Memory) UpdateMemberClout(ctx context.Context, circleID circle.ID, userID int32, newClout int) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	c, exists := r.circles[circleID]
	if !exists {
		return fmt.Errorf("circle not found with id: %d", circleID)
	}

	member, exists := c.Members[user.ID(userID)]
	if !exists {
		return fmt.Errorf("user not found in circle")
	}

	member.Clout = newClout
	return nil
}
