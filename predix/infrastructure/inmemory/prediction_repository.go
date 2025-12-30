package inmemory

import (
	"context"
	"errors"
	"sync"

	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/domain/prediction"
)

type PredictionRepository struct {
	mu          sync.RWMutex
	predictions map[prediction.ID]*prediction.Prediction
}

func NewPredictionRepository() *PredictionRepository {
	return &PredictionRepository{
		predictions: make(map[prediction.ID]*prediction.Prediction),
	}
}

func (r *PredictionRepository) Save(ctx context.Context, p *prediction.Prediction) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.predictions[p.ID] = p
	return nil
}

func (r *PredictionRepository) FindByID(ctx context.Context, id prediction.ID) (*prediction.Prediction, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	p, ok := r.predictions[id]
	if !ok {
		return nil, errors.New("prediction not found")
	}
	return p, nil
}

func (r *PredictionRepository) FindByCircleID(ctx context.Context, circleID circle.ID) ([]*prediction.Prediction, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	var result []*prediction.Prediction
	for _, p := range r.predictions {
		if p.CircleID == circleID {
			result = append(result, p)
		}
	}
	return result, nil
}
