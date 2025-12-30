package inmemory

import (
	"context"
	"errors"
	"sync"

	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
)

type CircleRepository struct {
	mu      sync.RWMutex
	circles map[circle.ID]*circle.Circle
	byCode  map[string]circle.ID
}

func NewCircleRepository() *CircleRepository {
	return &CircleRepository{
		circles: make(map[circle.ID]*circle.Circle),
		byCode:  make(map[string]circle.ID),
	}
}

func (r *CircleRepository) Save(ctx context.Context, c *circle.Circle) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.circles[c.ID] = c
	r.byCode[c.InviteCode] = c.ID
	return nil
}

func (r *CircleRepository) FindByID(ctx context.Context, id circle.ID) (*circle.Circle, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	c, ok := r.circles[id]
	if !ok {
		return nil, errors.New("circle not found")
	}
	return c, nil
}

func (r *CircleRepository) FindByInviteCode(ctx context.Context, code string) (*circle.Circle, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	id, ok := r.byCode[code]
	if !ok {
		return nil, errors.New("circle not found")
	}
	return r.circles[id], nil
}
