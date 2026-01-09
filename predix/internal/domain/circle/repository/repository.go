package repository

import (
	"context"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
)

// Repository defines the interface for persisting Circle entities.
type Repository interface {
	Save(ctx context.Context, c *circle.Circle) error
	FindByID(ctx context.Context, id circle.ID) (*circle.Circle, error)
	FindByUserID(ctx context.Context, userID string) ([]*circle.Circle, error)
	AddMember(ctx context.Context, circleID circle.ID, member *circle.Member) error
	UpdateMemberClout(ctx context.Context, circleID circle.ID, userID string, newClout int) error
	Delete(ctx context.Context, id circle.ID) error
}
