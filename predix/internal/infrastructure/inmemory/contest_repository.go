package inmemory

import (
	"context"
	"errors"
	"sync"

	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/domain/contest"
)

type ContestRepository struct {
	mu       sync.RWMutex
	contests map[contest.ID]*contest.Contest
}

func NewContestRepository() *ContestRepository {
	return &ContestRepository{
		contests: make(map[contest.ID]*contest.Contest),
	}
}

func (r *ContestRepository) Save(ctx context.Context, c *contest.Contest) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.contests[c.ID] = c
	return nil
}

func (r *ContestRepository) FindByID(ctx context.Context, id contest.ID) (*contest.Contest, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	c, ok := r.contests[id]
	if !ok {
		return nil, errors.New("contest not found")
	}
	return c, nil
}

func (r *ContestRepository) FindByCircleID(ctx context.Context, circleID circle.ID) ([]*contest.Contest, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	var result []*contest.Contest
	for _, c := range r.contests {
		if c.CircleID == circleID {
			result = append(result, c)
		}
	}
	return result, nil
}
