package repository

import (
	"context"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
)

// Repository defines the interface for persisting Contest entities.
type Repository interface {
	Save(ctx context.Context, c *contest.Contest) error
	FindByID(ctx context.Context, id contest.ID) (*contest.Contest, error)
	FindByCircleID(ctx context.Context, circleID circle.ID) ([]*contest.Contest, error)
	FindExpiredContests(ctx context.Context) ([]*contest.Contest, error)
	UpdateStatus(ctx context.Context, id contest.ID, status contest.Status) error
}
