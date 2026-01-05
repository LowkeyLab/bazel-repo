package service

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	circlerepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

type ContestRepository = repository.Repository

var (
	ErrNotContestCreator = errors.New("only the contest creator can resolve this contest")
	ErrInsufficientClout = errors.New("insufficient clout balance to make this prediction")
)

type Service struct {
	repo       repository.Repository
	circleRepo circlerepo.Repository
}

// NewService creates a service with repository interfaces.
func NewService(repo ContestRepository, circleRepo circlerepo.Repository) *Service {
	return &Service{
		repo:       repo,
		circleRepo: circleRepo,
	}
}

func (s *Service) CreateContest(ctx context.Context, circleIDs []circle.ID, creatorID user.ID, question string, options []string, expiresAt time.Time, minStake int) (*contest.Contest, error) {
	// Create domain entity (validation happens here)
	c, err := contest.New(circleIDs, creatorID, question, options, expiresAt, minStake)
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

func (s *Service) Predict(ctx context.Context, contestID contest.ID, circleID circle.ID, userID user.ID, optionID int, clout int) error {
	// Load contest from repository
	c, err := s.repo.FindByID(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	// Check if user has sufficient clout in the circle
	member, err := s.circleRepo.FindByID(ctx, circleID)
	if err != nil {
		return fmt.Errorf("failed to get circle: %w", err)
	}

	userMember, exists := member.Members[userID]
	if !exists {
		return errors.New("user is not a member of this circle")
	}

	if userMember.Clout < clout {
		return ErrInsufficientClout
	}

	// Use domain method to add prediction
	err = c.Predict(userID, optionID, clout)
	if err != nil {
		return err
	}

	// Deduct clout from member's balance
	newClout := userMember.Clout - clout
	err = s.circleRepo.UpdateMemberClout(ctx, circleID, int32(userID), newClout)
	if err != nil {
		return fmt.Errorf("failed to update member clout: %w", err)
	}

	// Save updated contest
	err = s.repo.Save(ctx, c)
	if err != nil {
		return fmt.Errorf("failed to save prediction: %w", err)
	}

	return nil
}

func (s *Service) ResolveContest(ctx context.Context, contestID contest.ID, resolverID user.ID, winningOptionID int) error {
	// Load contest from repository
	c, err := s.repo.FindByID(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	if c.CreatorID != resolverID {
		return ErrNotContestCreator
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

func (s *Service) GetContestsByCircleID(ctx context.Context, circleID circle.ID) ([]*contest.Contest, error) {
	return s.repo.FindByCircleID(ctx, circleID)
}
