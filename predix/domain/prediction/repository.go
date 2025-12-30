package prediction

import (
	"context"

	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
)

// Repository defines the interface for persisting Prediction entities.
type Repository interface {
	Save(ctx context.Context, prediction *Prediction) error
	FindByID(ctx context.Context, id ID) (*Prediction, error)
	FindByCircleID(ctx context.Context, circleID circle.ID) ([]*Prediction, error)
}
