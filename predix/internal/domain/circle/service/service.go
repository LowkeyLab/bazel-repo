package service

import (
	"context"
	"errors"
	"fmt"
	"time"

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

// EnrichedMember contains member data with username looked up from user repository.
type EnrichedMember struct {
	UserID   user.ID
	Username string
	Clout    int
}

// EnrichedCircle contains circle data with member usernames enriched.
type EnrichedCircle struct {
	ID        circle.ID
	Name      string
	CreatorID user.ID
	Members   []EnrichedMember
	CreatedAt time.Time
}

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

// GetCircleWithUsernames retrieves a circle and enriches member data with usernames.
func (s *Service) GetCircleWithUsernames(ctx context.Context, id circle.ID) (*EnrichedCircle, error) {
	circ, err := s.circleRepo.FindByID(ctx, id)
	if err != nil {
		return nil, err
	}

	return s.enrichCircle(ctx, circ)
}

// ListUserCirclesWithUsernames retrieves all circles for a user with member usernames enriched.
func (s *Service) ListUserCirclesWithUsernames(ctx context.Context, userID user.ID) ([]*EnrichedCircle, error) {
	circles, err := s.circleRepo.FindByUserID(ctx, int32(userID))
	if err != nil {
		return nil, fmt.Errorf("failed to list user circles: %w", err)
	}

	enriched := make([]*EnrichedCircle, len(circles))
	for i, circ := range circles {
		enriched[i], err = s.enrichCircle(ctx, circ)
		if err != nil {
			return nil, err
		}
	}

	return enriched, nil
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

// enrichCircle looks up usernames for all members in a circle.
func (s *Service) enrichCircle(ctx context.Context, circ *circle.Circle) (*EnrichedCircle, error) {
	members := make([]EnrichedMember, 0, len(circ.Members))

	for _, member := range circ.Members {
		u, err := s.userRepo.FindByID(ctx, member.UserID)
		if err != nil {
			return nil, fmt.Errorf("failed to get user %d: %w", member.UserID, err)
		}

		members = append(members, EnrichedMember{
			UserID:   member.UserID,
			Username: u.Username,
			Clout:    member.Clout,
		})
	}

	return &EnrichedCircle{
		ID:        circ.ID,
		Name:      circ.Name,
		CreatorID: circ.CreatorID,
		Members:   members,
		CreatedAt: circ.CreatedAt,
	}, nil
}
