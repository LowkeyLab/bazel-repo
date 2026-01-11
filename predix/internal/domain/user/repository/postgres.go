package repository

import (
	"context"
	"database/sql"
	"errors"
	"fmt"

	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Postgres is a PostgreSQL implementation of user repository interfaces.
type Postgres struct {
	db      *sql.DB
	queries *db.Queries
}

// NewPostgres creates a new Postgres repository.
func NewPostgres(sqlDB *sql.DB) *Postgres {
	return &Postgres{
		db:      sqlDB,
		queries: db.New(sqlDB),
	}
}

// Save persists a User to the database.
func (r *Postgres) Save(ctx context.Context, u *user.User) error {
	result, err := r.queries.CreateUser(ctx, db.CreateUserParams{
		Username:     u.Username,
		PasswordHash: u.PasswordHash,
		Role:         db.UserRole(u.Role),
	})
	if err != nil {
		return fmt.Errorf("failed to save user: %w", err)
	}
	// Update the user with the generated ID
	u.ID = user.ID(result.ID)
	return nil
}

// FindByID retrieves a User by its ID.
func (r *Postgres) FindByID(ctx context.Context, id user.ID) (*user.User, error) {
	dbUser, err := r.queries.GetUser(ctx, int32(id))
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, fmt.Errorf("failed to find user by id: %w", ErrNotFound)
		}
		return nil, fmt.Errorf("failed to find user by id: %w", err)
	}

	return toDomainUser(&dbUser), nil
}

// FindByUsername retrieves a User by username.
func (r *Postgres) FindByUsername(ctx context.Context, username string) (*user.User, error) {
	dbUser, err := r.queries.GetUserByUsername(ctx, username)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, fmt.Errorf("failed to find user by username: %w", ErrNotFound)
		}
		return nil, fmt.Errorf("failed to find user by username: %w", err)
	}

	return toDomainUser(&dbUser), nil
}

func toDomainUser(dbUser *db.User) *user.User {
	return &user.User{
		ID:           user.ID(dbUser.ID),
		Username:     dbUser.Username,
		PasswordHash: dbUser.PasswordHash,
		Role:         user.Role(dbUser.Role),
	}
}
