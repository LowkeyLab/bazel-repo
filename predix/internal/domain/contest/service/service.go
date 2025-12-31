package service

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

type Service struct {
	repo repository.Repository
}

func NewService(pool *pgxpool.Pool) *Service {
	return &Service{
		repo: repository.NewPostgres(pool),
	}
}

func (s *Service) CreateContest(ctx context.Context, circleIDs []circle.ID, creatorID user.ID, question string, options []string, expiresAt time.Time) (*contest.Contest, error) {
	// Create domain entity (validation happens here)
	c, err := contest.New(circleIDs, creatorID, question, options, expiresAt)
	if err != nil {
		return nil, err
	}

	// Persist using repository
	err = s.repo.Save(ctx, c)
	if err != nil {
		return nil, err
	}

	return c, nil
}

func (s *Service) Predict(ctx context.Context, contestID contest.ID, userID user.ID, optionID int, clout int) error {
	// Load contest from repository
	c, err := s.repo.FindByID(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	// Use domain method to add prediction
	err = c.Predict(userID, optionID, clout)
	if err != nil {
		return err
	}

	// Save updated contest
	err = s.repo.Save(ctx, c)
	if err != nil {
		return fmt.Errorf("failed to save prediction: %w", err)
	}

	return nil
}

func (s *Service) ResolveContest(ctx context.Context, contestID contest.ID, winningOptionID int) error {
	// Load contest from repository
	c, err := s.repo.FindByID(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	// Use domain method to resolve
	err = c.Resolve(winningOptionID)
	if err != nil {
		return err
	}

	// Save updated contest
	err = s.repo.Save(ctx, c)
	if err != nil {
		return fmt.Errorf("failed to save resolved contest: %w", err)
	}

	return nil
}

func (s *Service) GetContest(ctx context.Context, id contest.ID) (*contest.Contest, error) {
	return s.repo.FindByID(ctx, id)
}
