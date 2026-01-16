package repository

import (
	"context"
	"database/sql"
	"fmt"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Postgres is a PostgreSQL implementation of circle repository interfaces.
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

// q returns the queries object, using the transaction from the context if present.
func (r *Postgres) q(ctx context.Context) *db.Queries {
	if tx, ok := db.GetTx(ctx); ok {
		return r.queries.WithTx(tx)
	}
	return r.queries
}

// Save persists a Circle and its members to the database.
func (r *Postgres) Save(ctx context.Context, c *circle.Circle) error {
	// Check if we are already in a transaction
	tx, hasTx := db.GetTx(ctx)
	if !hasTx {
		// No external transaction, start one
		var err error
		tx, err = r.db.BeginTx(ctx, nil)
		if err != nil {
			return fmt.Errorf("failed to begin transaction: %w", err)
		}
		defer tx.Rollback()
	}

	qtx := r.queries.WithTx(tx)

	// Save circle
	result, err := qtx.CreateCircle(ctx, db.CreateCircleParams{
		Name:      c.Name,
		CreatorID: uuid.UUID(c.CreatorID),
		CreatedAt: c.CreatedAt,
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
			UserID:   uuid.UUID(member.UserID),
			Clout:    int32(member.Clout),
		})
		if err != nil {
			return fmt.Errorf("failed to save circle member: %w", err)
		}
	}

	if !hasTx {
		if err := tx.Commit(); err != nil {
			return fmt.Errorf("failed to commit transaction: %w", err)
		}
	}

	return nil
}

// AddMember persists a single member to an existing circle.
func (r *Postgres) AddMember(ctx context.Context, circleID circle.ID, member *circle.Member) error {
	if member == nil {
		return fmt.Errorf("member cannot be nil")
	}

	err := r.q(ctx).AddCircleMember(ctx, db.AddCircleMemberParams{
		CircleID: int32(circleID),
		UserID:   uuid.UUID(member.UserID),
		Clout:    int32(member.Clout),
	})
	if err != nil {
		return fmt.Errorf("failed to save circle member: %w", err)
	}

	return nil
}

// FindByID retrieves a Circle and its members by ID.
func (r *Postgres) FindByID(ctx context.Context, id circle.ID) (*circle.Circle, error) {
	dbCircle, err := r.q(ctx).GetCircle(ctx, int32(id))
	if err != nil {
		return nil, fmt.Errorf("failed to find circle by id: %w", err)
	}

	// Load members
	dbMembers, err := r.q(ctx).ListCircleMembers(ctx, int32(id))
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
		CreatorID: user.ID(dbCircle.CreatorID),
		CreatedAt: dbCircle.CreatedAt,
		Members:   members,
	}, nil
}

// FindByUserID retrieves all circles for a given user, ordered by creation date.
func (r *Postgres) FindByUserID(ctx context.Context, userID user.ID) ([]*circle.Circle, error) {
	dbCircles, err := r.q(ctx).ListUserCircles(ctx, uuid.UUID(userID))
	if err != nil {
		return nil, fmt.Errorf("failed to find circles by user id: %w", err)
	}

	var circles []*circle.Circle
	for _, dbCircle := range dbCircles {
		// Load members for each circle
		dbMembers, err := r.q(ctx).ListCircleMembers(ctx, dbCircle.ID)
		if err != nil {
			return nil, fmt.Errorf("failed to load circle members for circle %d: %w", dbCircle.ID, err)
		}

		members := make(map[user.ID]*circle.Member)
		for _, dbMember := range dbMembers {
			memberUserID := user.ID(dbMember.UserID)
			members[memberUserID] = &circle.Member{
				UserID: memberUserID,
				Clout:  int(dbMember.Clout),
			}
		}

		circles = append(circles, &circle.Circle{
			ID:        circle.ID(dbCircle.ID),
			Name:      dbCircle.Name,
			CreatorID: user.ID(dbCircle.CreatorID),
			CreatedAt: dbCircle.CreatedAt,
			Members:   members,
		})
	}

	return circles, nil
}

// Delete removes a circle and its related data.
func (r *Postgres) Delete(ctx context.Context, id circle.ID) error {
	if err := r.q(ctx).DeleteCircle(ctx, int32(id)); err != nil {
		return fmt.Errorf("failed to delete circle: %w", err)
	}

	return nil
}

// UpdateMemberClout updates a member's clout balance in a circle.
func (r *Postgres) UpdateMemberClout(ctx context.Context, circleID circle.ID, userID user.ID, newClout int) error {
	if err := r.q(ctx).UpdateCircleMemberClout(ctx, db.UpdateCircleMemberCloutParams{
		CircleID: int32(circleID),
		UserID:   uuid.UUID(userID),
		Clout:    int32(newClout),
	}); err != nil {
		return fmt.Errorf("failed to update member clout: %w", err)
	}

	return nil
}
