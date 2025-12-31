package repository

import (
	"context"
	"errors"
	"fmt"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Postgres is a PostgreSQL implementation of the Repository interface.
type Postgres struct {
	pool    *pgxpool.Pool
	queries *db.Queries
}

// NewPostgres creates a new Postgres repository.
func NewPostgres(pool *pgxpool.Pool) *Postgres {
	return &Postgres{
		pool:    pool,
		queries: db.New(pool),
	}
}

// Save persists a User to the database.
func (r *Postgres) Save(ctx context.Context, u *user.User) error {
	result, err := r.queries.CreateUser(ctx, db.CreateUserParams{
		Username:     u.Username,
		PasswordHash: u.PasswordHash,
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
		if errors.Is(err, pgx.ErrNoRows) {
			return nil, fmt.Errorf("failed to find user by id: %w", ErrNotFound)
		}
		return nil, fmt.Errorf("failed to find user by id: %w", err)
	}

	return &user.User{
		ID:           user.ID(dbUser.ID),
		Username:     dbUser.Username,
		PasswordHash: dbUser.PasswordHash,
	}, nil
}

// FindByUsername retrieves a User by username.
func (r *Postgres) FindByUsername(ctx context.Context, username string) (*user.User, error) {
	dbUser, err := r.queries.GetUserByUsername(ctx, username)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			return nil, fmt.Errorf("failed to find user by username: %w", ErrNotFound)
		}
		return nil, fmt.Errorf("failed to find user by username: %w", err)
	}

	return &user.User{
		ID:           user.ID(dbUser.ID),
		Username:     dbUser.Username,
		PasswordHash: dbUser.PasswordHash,
	}, nil
}
