package user

import "context"

// Repository defines the interface for persisting User entities.
type Repository interface {
	Save(ctx context.Context, user *User) error
	FindByID(ctx context.Context, id ID) (*User, error)
	FindByEmail(ctx context.Context, email string) (*User, error)
}
