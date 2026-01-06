package repository

import (
	"context"
	"fmt"
	"sync"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Memory is an in-memory implementation of the Repository interface.
type Memory struct {
	mu      sync.RWMutex
	users   map[user.ID]*user.User
	nameIdx map[string]user.ID
	nextID  user.ID
}

// NewMemory creates a new in-memory user repository.
func NewMemory() *Memory {
	return &Memory{
		users:   make(map[user.ID]*user.User),
		nameIdx: make(map[string]user.ID),
		nextID:  1,
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

	// Check if username already exists (for a different user)
	if existingID, exists := r.nameIdx[u.Username]; exists && existingID != u.ID {
		return fmt.Errorf("user with username %s already exists", u.Username)
	}

	// Store user
	userCopy := &user.User{
		ID:           u.ID,
		Username:     u.Username,
		PasswordHash: u.PasswordHash,
		Role:         u.Role,
	}

	r.users[u.ID] = userCopy
	r.nameIdx[u.Username] = u.ID

	return nil
}

// FindByID retrieves a User by its ID.
func (r *Memory) FindByID(ctx context.Context, id user.ID) (*user.User, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	u, exists := r.users[id]
	if !exists {
		return nil, fmt.Errorf("%w: id %d", ErrNotFound, id)
	}

	// Return a copy to avoid external mutations
	return &user.User{
		ID:           u.ID,
		Username:     u.Username,
		PasswordHash: u.PasswordHash,
		Role:         u.Role,
	}, nil
}

// FindByUsername retrieves a User by username.
func (r *Memory) FindByUsername(ctx context.Context, username string) (*user.User, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	id, exists := r.nameIdx[username]
	if !exists {
		return nil, fmt.Errorf("%w: username %s", ErrNotFound, username)
	}

	u := r.users[id]

	// Return a copy to avoid external mutations
	return &user.User{
		ID:           u.ID,
		Username:     u.Username,
		PasswordHash: u.PasswordHash,
		Role:         u.Role,
	}, nil
}
