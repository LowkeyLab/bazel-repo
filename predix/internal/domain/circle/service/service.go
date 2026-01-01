package service

import (
	"context"
	"errors"
	"fmt"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
)

type CircleRepository = repository.Repository

type Service struct {
	circleRepo repository.Repository
	userRepo   userrepo.Repository
}

// NewService creates a service with a repository interface.
func NewService(circleRepo CircleRepository, userRepo userrepo.Repository) *Service {
	return &Service{
		circleRepo: circleRepo,
		userRepo:   userRepo,
	}
}

var ErrNotCircleOwner = errors.New("only the circle creator or an admin can delete this circle")

func (s *Service) CreateCircle(ctx context.Context, name string, creatorID user.ID) (*circle.Circle, error) {
	// Create domain entity (validation happens here)
	c, err := circle.New(name, creatorID)
	if err != nil {
		return nil, err
	}

	// Persist using repository
	err = s.circleRepo.Save(ctx, c)
	if err != nil {
		return nil, err
	}

	return c, nil
}

func (s *Service) AddMember(ctx context.Context, circleID circle.ID, userID user.ID) error {
	current, err := s.circleRepo.FindByID(ctx, circleID)
	if err != nil {
		return fmt.Errorf("failed to get circle: %w", err)
	}

	if _, exists := current.Members[userID]; exists {
		return nil
	}

	current.AddMember(userID)
	member := current.Members[userID]

	if err := s.circleRepo.AddMember(ctx, circleID, member); err != nil {
		return fmt.Errorf("failed to add member: %w", err)
	}

	return nil
}

func (s *Service) GetCircle(ctx context.Context, id circle.ID) (*circle.Circle, error) {
	return s.circleRepo.FindByID(ctx, id)
}

func (s *Service) ListUserCircles(ctx context.Context, userID user.ID) ([]*circle.Circle, error) {
	circles, err := s.circleRepo.FindByUserID(ctx, int32(userID))
	if err != nil {
		return nil, fmt.Errorf("failed to list user circles: %w", err)
	}

	return circles, nil
}

func (s *Service) DeleteCircle(ctx context.Context, id circle.ID, requesterID user.ID) error {
	circ, err := s.circleRepo.FindByID(ctx, id)
	if err != nil {
		return fmt.Errorf("failed to get circle: %w", err)
	}

	requester, err := s.userRepo.FindByID(ctx, requesterID)
	if err != nil {
		return fmt.Errorf("failed to get user: %w", err)
	}

	if circ.CreatorID != requesterID && requester.Role != user.RoleAdmin {
		return ErrNotCircleOwner
	}

	if err := s.circleRepo.Delete(ctx, id); err != nil {
		return fmt.Errorf("failed to delete circle: %w", err)
	}

	return nil
}
