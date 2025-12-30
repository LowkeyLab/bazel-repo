package contest

import (
	"context"
	"fmt"
	"time"

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

func (s *Service) CreateContest(ctx context.Context, circleID circle.ID, creatorID user.ID, question string, options []string, expiresAt time.Time) (*Contest, error) {
	// 1. Create domain entity (validation logic is here)
	c, err := New(circleID, creatorID, question, options, expiresAt)
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

func (s *Service) Predict(ctx context.Context, contestID ID, userID user.ID, optionID int, clout int) error {
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return fmt.Errorf("failed to begin transaction: %w", err)
	}
	defer tx.Rollback(ctx)

	qtx := s.queries.WithTx(tx)

	c, err := qtx.GetContest(ctx, uuid.UUID(contestID))
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	if c.Status != string(StatusOpen) {
		return fmt.Errorf("contest is not open")
	}

	_, err = qtx.CreatePrediction(ctx, db.CreatePredictionParams{
		ID:        uuid.New(),
		ContestID: uuid.UUID(contestID),
		UserID:    uuid.UUID(userID),
		OptionID:  int32(optionID),
		Clout:     int32(clout),
		CreatedAt: pgtype.Timestamp{Time: time.Now(), Valid: true},
	})
	if err != nil {
		return fmt.Errorf("failed to create prediction: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return fmt.Errorf("failed to commit transaction: %w", err)
	}
	return nil
}

func (s *Service) ResolveContest(ctx context.Context, contestID ID, winningOptionID int) error {
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return fmt.Errorf("failed to begin transaction: %w", err)
	}
	defer tx.Rollback(ctx)

	qtx := s.queries.WithTx(tx)

	err = qtx.UpdateContestStatus(ctx, db.UpdateContestStatusParams{
		ID:             uuid.UUID(contestID),
		Status:         string(StatusResolved),
		ResultOptionID: pgtype.Int4{Int32: int32(winningOptionID), Valid: true},
	})
	if err != nil {
		return fmt.Errorf("failed to update contest status: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return fmt.Errorf("failed to commit transaction: %w", err)
	}
	return nil
}