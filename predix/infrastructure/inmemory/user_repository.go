package inmemory

import (
	"context"
	"errors"
	"sync"

	"github.com/lowkeylab/bazel-repo/predix/domain/user"
)

type UserRepository struct {
	mu      sync.RWMutex
	users   map[user.ID]*user.User
	byEmail map[string]user.ID
}

func NewUserRepository() *UserRepository {
	return &UserRepository{
		users:   make(map[user.ID]*user.User),
		byEmail: make(map[string]user.ID),
	}
}

func (r *UserRepository) Save(ctx context.Context, u *user.User) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.users[u.ID] = u
	r.byEmail[u.Email] = u.ID
	return nil
}

func (r *UserRepository) FindByID(ctx context.Context, id user.ID) (*user.User, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	u, ok := r.users[id]
	if !ok {
		return nil, errors.New("user not found")
	}
	return u, nil
}

func (r *UserRepository) FindByEmail(ctx context.Context, email string) (*user.User, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	id, ok := r.byEmail[email]
	if !ok {
		return nil, errors.New("user not found")
	}
	return r.users[id], nil
}
