package service

import (
	"context"
	"fmt"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

type CircleRepository = repository.Repository

type Service struct {
	repo repository.Repository
}

// NewService creates a service with a repository interface.
func NewService(repo CircleRepository) *Service {
	return &Service{
		repo: repo,
	}
}

// NewServiceWithPool creates a service with a PostgreSQL pool (backward compatible).
func NewServiceWithPool(pool *pgxpool.Pool) *Service {
	return &Service{
		repo: repository.NewPostgres(pool),
	}
}

func (s *Service) CreateCircle(ctx context.Context, name string, creatorID user.ID) (*circle.Circle, error) {
	// Create domain entity (validation happens here)
	c, err := circle.New(name, creatorID)
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

func (s *Service) AddMember(ctx context.Context, circleID circle.ID, userID user.ID) error {
	current, err := s.repo.FindByID(ctx, circleID)
	if err != nil {
		return fmt.Errorf("failed to get circle: %w", err)
	}

	if _, exists := current.Members[userID]; exists {
		return nil
	}

	current.AddMember(userID)
	member := current.Members[userID]

	if err := s.repo.AddMember(ctx, circleID, member); err != nil {
		return fmt.Errorf("failed to add member: %w", err)
	}

	return nil
}

func (s *Service) GetCircle(ctx context.Context, id circle.ID) (*circle.Circle, error) {
	return s.repo.FindByID(ctx, id)
}
