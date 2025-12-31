package service

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

type Service struct {
	pool    *pgxpool.Pool
	queries *db.Queries
}

func NewService(pool *pgxpool.Pool) *Service {
	return &Service{
		pool:    pool,
		queries: db.New(pool),
	}
}

func (s *Service) CreateCircle(ctx context.Context, name string, creatorID user.ID) (*circle.Circle, error) {
	// Domain logic
	c, err := circle.New(name, creatorID)
	if err != nil {
		return nil, err
	}

	// Persistence
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to begin transaction: %w", err)
	}
	defer tx.Rollback(ctx)

	qtx := s.queries.WithTx(tx)

	_, err = qtx.CreateCircle(ctx, db.CreateCircleParams{
		ID:         uuid.UUID(c.ID),
		Name:       c.Name,
		InviteCode: c.InviteCode,
		CreatedAt:  pgtype.Timestamp{Time: c.CreatedAt, Valid: true},
	})
	if err != nil {
		return nil, fmt.Errorf("failed to create circle: %w", err)
	}

	for _, m := range c.Members {
		err = qtx.AddCircleMember(ctx, db.AddCircleMemberParams{
			CircleID: uuid.UUID(c.ID),
			UserID:   uuid.UUID(m.UserID),
			Clout:    int32(m.Clout),
		})
		if err != nil {
			return nil, fmt.Errorf("failed to add member: %w", err)
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return nil, fmt.Errorf("failed to commit transaction: %w", err)
	}

	return c, nil
}
