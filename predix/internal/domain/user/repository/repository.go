package repository

import (
	"context"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Repository defines the interface for persisting User entities.
type Repository interface {
	Save(ctx context.Context, u *user.User) error
	FindByID(ctx context.Context, id user.ID) (*user.User, error)
	FindByEmail(ctx context.Context, email string) (*user.User, error)
}
