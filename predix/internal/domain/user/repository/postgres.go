package repository

import (
	"context"
	"fmt"

	"github.com/google/uuid"
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
	_, err := r.queries.CreateUser(ctx, db.CreateUserParams{
		ID:    uuid.UUID(u.ID),
		Name:  u.Name,
		Email: u.Email,
	})
	if err != nil {
		return fmt.Errorf("failed to save user: %w", err)
	}
	return nil
}

// FindByID retrieves a User by its ID.
func (r *Postgres) FindByID(ctx context.Context, id user.ID) (*user.User, error) {
	dbUser, err := r.queries.GetUser(ctx, uuid.UUID(id))
	if err != nil {
		return nil, fmt.Errorf("failed to find user by id: %w", err)
	}

	return &user.User{
		ID:    user.ID(dbUser.ID),
		Name:  dbUser.Name,
		Email: dbUser.Email,
	}, nil
}

// FindByEmail retrieves a User by email address.
func (r *Postgres) FindByEmail(ctx context.Context, email string) (*user.User, error) {
	dbUser, err := r.queries.GetUserByEmail(ctx, email)
	if err != nil {
		return nil, fmt.Errorf("failed to find user by email: %w", err)
	}

	return &user.User{
		ID:    user.ID(dbUser.ID),
		Name:  dbUser.Name,
		Email: dbUser.Email,
	}, nil
}
