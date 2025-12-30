package circle

import "context"

// Repository defines the interface for persisting Circle entities.
type Repository interface {
	Save(ctx context.Context, circle *Circle) error
	FindByID(ctx context.Context, id ID) (*Circle, error)
	FindByInviteCode(ctx context.Context, code string) (*Circle, error)
}
