package core

import (
	"context"
	"fmt"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/core/db"
)

type ContestService struct {
	pool    *pgxpool.Pool
	queries *db.Queries
}

func NewContestService(pool *pgxpool.Pool) *ContestService {
	return &ContestService{
		pool:    pool,
		queries: db.New(pool),
	}
}

func (s *ContestService) CreateContest(ctx context.Context, circleID circle.ID, creatorID user.ID, question string, options []string, expiresAt time.Time) (*contest.Contest, error) {
	// 1. Create domain entity (validation logic is here)
	c, err := contest.New(circleID, creatorID, question, options, expiresAt)
	if err != nil {
		return nil, err
	}

	// 2. Persist in transaction
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to begin transaction: %w", err)
	}
	defer tx.Rollback(ctx)

	qtx := s.queries.WithTx(tx)

	// Create Contest
	_, err = qtx.CreateContest(ctx, db.CreateContestParams{
		ID:        uuid.UUID(c.ID),
		CircleID:  uuid.UUID(c.CircleID),
		CreatorID: uuid.UUID(c.CreatorID),
		Question:  c.Question,
		Status:    string(c.Status),
		CreatedAt: pgtype.Timestamp{Time: c.CreatedAt, Valid: true},
		ExpiresAt: pgtype.Timestamp{Time: c.ExpiresAt, Valid: true},
	})
	if err != nil {
		return nil, fmt.Errorf("failed to insert contest: %w", err)
	}

	// Create Options
	for _, opt := range c.Options {
		err = qtx.CreateOption(ctx, db.CreateOptionParams{
			ContestID: uuid.UUID(c.ID),
			OptionID:  int32(opt.ID),
			Text:      opt.Text,
		})
		if err != nil {
			return nil, fmt.Errorf("failed to insert option %d: %w", opt.ID, err)
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return nil, fmt.Errorf("failed to commit transaction: %w", err)
	}

	return c, nil
}
