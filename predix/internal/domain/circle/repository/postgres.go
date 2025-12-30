package repository

import (
	"context"
	"fmt"

	"github.com/google/uuid"
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
	_, err = qtx.CreateCircle(ctx, db.CreateCircleParams{
		ID:         uuid.UUID(c.ID),
		Name:       c.Name,
		InviteCode: c.InviteCode,
		CreatedAt:  pgtype.Timestamp{Time: c.CreatedAt, Valid: true},
	})
	if err != nil {
		return fmt.Errorf("failed to save circle: %w", err)
	}

	// Save members
	for _, member := range c.Members {
		err = qtx.AddCircleMember(ctx, db.AddCircleMemberParams{
			CircleID: uuid.UUID(c.ID),
			UserID:   uuid.UUID(member.UserID),
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

// FindByID retrieves a Circle and its members by ID.
func (r *Postgres) FindByID(ctx context.Context, id circle.ID) (*circle.Circle, error) {
	dbCircle, err := r.queries.GetCircle(ctx, uuid.UUID(id))
	if err != nil {
		return nil, fmt.Errorf("failed to find circle by id: %w", err)
	}

	// Load members
	dbMembers, err := r.queries.ListCircleMembers(ctx, uuid.UUID(id))
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
		ID:         circle.ID(dbCircle.ID),
		Name:       dbCircle.Name,
		InviteCode: dbCircle.InviteCode,
		CreatedAt:  dbCircle.CreatedAt.Time,
		Members:    members,
	}, nil
}

// FindByInviteCode retrieves a Circle by its invite code.
func (r *Postgres) FindByInviteCode(ctx context.Context, code string) (*circle.Circle, error) {
	dbCircle, err := r.queries.GetCircleByInviteCode(ctx, code)
	if err != nil {
		return nil, fmt.Errorf("failed to find circle by invite code: %w", err)
	}

	// Load members
	dbMembers, err := r.queries.ListCircleMembers(ctx, dbCircle.ID)
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
		ID:         circle.ID(dbCircle.ID),
		Name:       dbCircle.Name,
		InviteCode: dbCircle.InviteCode,
		CreatedAt:  dbCircle.CreatedAt.Time,
		Members:    members,
	}, nil
}
