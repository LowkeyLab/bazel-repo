package user

import (
	"context"
	"fmt"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
)

// PostgresRepository is a PostgreSQL implementation of the Repository interface.
type PostgresRepository struct {
	pool    *pgxpool.Pool
	queries *db.Queries
}

// NewPostgresRepository creates a new PostgresRepository.
func NewPostgresRepository(pool *pgxpool.Pool) *PostgresRepository {
	return &PostgresRepository{
		pool:    pool,
		queries: db.New(pool),
	}
}

// Save persists a User to the database.
func (r *PostgresRepository) Save(ctx context.Context, user *User) error {
	_, err := r.queries.CreateUser(ctx, db.CreateUserParams{
		ID:    uuid.UUID(user.ID),
		Name:  user.Name,
		Email: user.Email,
	})
	if err != nil {
		return fmt.Errorf("failed to save user: %w", err)
	}
	return nil
}

// FindByID retrieves a User by its ID.
func (r *PostgresRepository) FindByID(ctx context.Context, id ID) (*User, error) {
	dbUser, err := r.queries.GetUser(ctx, uuid.UUID(id))
	if err != nil {
		return nil, fmt.Errorf("failed to find user by id: %w", err)
	}

	return &User{
		ID:    ID(dbUser.ID),
		Name:  dbUser.Name,
		Email: dbUser.Email,
	}, nil
}

// FindByEmail retrieves a User by email address.
func (r *PostgresRepository) FindByEmail(ctx context.Context, email string) (*User, error) {
	dbUser, err := r.queries.GetUserByEmail(ctx, email)
	if err != nil {
		return nil, fmt.Errorf("failed to find user by email: %w", err)
	}

	return &User{
		ID:    ID(dbUser.ID),
		Name:  dbUser.Name,
		Email: dbUser.Email,
	}, nil
}
