package service

import (
	"context"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
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
