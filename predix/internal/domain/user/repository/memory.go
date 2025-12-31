package repository

import (
	"context"
	"fmt"
	"sync"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Memory is an in-memory implementation of the Repository interface.
type Memory struct {
	mu       sync.RWMutex
	users    map[user.ID]*user.User
	emailIdx map[string]user.ID
	nextID   user.ID
}

// NewMemory creates a new in-memory user repository.
func NewMemory() *Memory {
	return &Memory{
		users:    make(map[user.ID]*user.User),
		emailIdx: make(map[string]user.ID),
		nextID:   1,
	}
}

// Save persists a User to memory.
func (r *Memory) Save(ctx context.Context, u *user.User) error {
	if u == nil {
		return fmt.Errorf("user cannot be nil")
	}

	r.mu.Lock()
	defer r.mu.Unlock()

	// If no ID, assign a new one
	if u.ID == 0 {
		u.ID = r.nextID
		r.nextID++
	}

	// Check if email already exists (for a different user)
	if existingID, exists := r.emailIdx[u.Email]; exists && existingID != u.ID {
		return fmt.Errorf("user with email %s already exists", u.Email)
	}

	// Store user
	userCopy := &user.User{
		ID:    u.ID,
		Name:  u.Name,
		Email: u.Email,
	}

	r.users[u.ID] = userCopy
	r.emailIdx[u.Email] = u.ID

	return nil
}

// FindByID retrieves a User by its ID.
func (r *Memory) FindByID(ctx context.Context, id user.ID) (*user.User, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	u, exists := r.users[id]
	if !exists {
		return nil, fmt.Errorf("user not found with id: %d", id)
	}

	// Return a copy to avoid external mutations
	return &user.User{
		ID:    u.ID,
		Name:  u.Name,
		Email: u.Email,
	}, nil
}

// FindByEmail retrieves a User by email address.
func (r *Memory) FindByEmail(ctx context.Context, email string) (*user.User, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	id, exists := r.emailIdx[email]
	if !exists {
		return nil, fmt.Errorf("user not found with email: %s", email)
	}

	u := r.users[id]

	// Return a copy to avoid external mutations
	return &user.User{
		ID:    u.ID,
		Name:  u.Name,
		Email: u.Email,
	}, nil
}
