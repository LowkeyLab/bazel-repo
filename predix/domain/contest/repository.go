package contest

import (
	"context"

	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
)

// Repository defines the interface for persisting Contest entities.
type Repository interface {
	Save(ctx context.Context, contest *Contest) error
	FindByID(ctx context.Context, id ID) (*Contest, error)
	FindByCircleID(ctx context.Context, circleID circle.ID) ([]*Contest, error)
}
