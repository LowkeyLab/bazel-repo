package repository

import (
	"context"
	"fmt"

	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
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

// Save persists a Circle and its members to the database.
func (r *Postgres) Save(ctx context.Context, c *circle.Circle) error {
	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return fmt.Errorf("failed to begin transaction: %w", err)
	}
	defer tx.Rollback(ctx)

	qtx := r.queries.WithTx(tx)

	// Save circle
	result, err := qtx.CreateCircle(ctx, db.CreateCircleParams{
		Name:      c.Name,
		CreatedAt: pgtype.Timestamp{Time: c.CreatedAt, Valid: true},
	})
	if err != nil {
		return fmt.Errorf("failed to save circle: %w", err)
	}

	// Update circle with generated ID
	c.ID = circle.ID(result.ID)

	// Save members
	for _, member := range c.Members {
		err = qtx.AddCircleMember(ctx, db.AddCircleMemberParams{
			CircleID: int32(c.ID),
			UserID:   int32(member.UserID),
			Clout:    int32(member.Clout),
		})
		if err != nil {
			return fmt.Errorf("failed to save circle member: %w", err)
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return fmt.Errorf("failed to commit transaction: %w", err)
	}

	return nil
}

// AddMember persists a single member to an existing circle.
func (r *Postgres) AddMember(ctx context.Context, circleID circle.ID, member *circle.Member) error {
	if member == nil {
		return fmt.Errorf("member cannot be nil")
	}

	err := r.queries.AddCircleMember(ctx, db.AddCircleMemberParams{
		CircleID: int32(circleID),
		UserID:   int32(member.UserID),
		Clout:    int32(member.Clout),
	})
	if err != nil {
		return fmt.Errorf("failed to save circle member: %w", err)
	}

	return nil
}

// FindByID retrieves a Circle and its members by ID.
func (r *Postgres) FindByID(ctx context.Context, id circle.ID) (*circle.Circle, error) {
	dbCircle, err := r.queries.GetCircle(ctx, int32(id))
	if err != nil {
		return nil, fmt.Errorf("failed to find circle by id: %w", err)
	}

	// Load members
	dbMembers, err := r.queries.ListCircleMembers(ctx, int32(id))
	if err != nil {
		return nil, fmt.Errorf("failed to load circle members: %w", err)
	}

	members := make(map[user.ID]*circle.Member)
	for _, dbMember := range dbMembers {
		userID := user.ID(dbMember.UserID)
		members[userID] = &circle.Member{
			UserID: userID,
			Clout:  int(dbMember.Clout),
		}
	}

	return &circle.Circle{
		ID:        circle.ID(dbCircle.ID),
		Name:      dbCircle.Name,
		CreatedAt: dbCircle.CreatedAt.Time,
		Members:   members,
	}, nil
}
