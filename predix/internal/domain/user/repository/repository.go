package repository

import (
	"context"
	"errors"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// ErrNotFound is returned when a user cannot be located.
var ErrNotFound = errors.New("user not found")

// Repository defines the interface for persisting User entities.
type Repository interface {
	Save(ctx context.Context, u *user.User) error
	FindByID(ctx context.Context, id user.ID) (*user.User, error)
	FindByUsername(ctx context.Context, username string) (*user.User, error)
}
