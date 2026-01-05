package service

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	contestrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
)

type (
	CircleRepository  = repository.Repository
	ContestRepository = contestrepo.Repository
)

type Service struct {
	circleRepo  repository.Repository
	userRepo    userrepo.Repository
	contestRepo contestrepo.Repository
	clock       clock.Clock
}

// NewService creates a service with a repository interface.
func NewService(circleRepo CircleRepository, userRepo userrepo.Repository, contestRepo ContestRepository, clk clock.Clock) *Service {
	return &Service{
		circleRepo:  circleRepo,
		userRepo:    userRepo,
		contestRepo: contestRepo,
		clock:       clk,
	}
}

var (
	ErrNotCircleOwner    = errors.New("only the circle creator or an admin can delete this circle")
	ErrInsufficientClout = errors.New("insufficient clout balance to make this prediction")
	ErrUserNotInCircle   = errors.New("user is not a member of this circle")
	ErrNotContestCreator = errors.New("only the contest creator can resolve this contest")
)

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

// JoinCircle allows a user to join a circle themselves.
func (s *Service) JoinCircle(ctx context.Context, circleID circle.ID, userID user.ID) error {
	current, err := s.circleRepo.FindByID(ctx, circleID)
	if err != nil {
		return fmt.Errorf("failed to get circle: %w", err)
	}

	if _, exists := current.Members[userID]; exists {
		return errors.New("user is already a member of this circle")
	}

	current.AddMember(userID)
	member := current.Members[userID]

	if err := s.circleRepo.AddMember(ctx, circleID, member); err != nil {
		return fmt.Errorf("failed to join circle: %w", err)
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

// Predict handles the prediction workflow: validate clout, deduct it, and record the prediction.
func (s *Service) Predict(ctx context.Context, contestID contest.ID, userID user.ID, optionID int, clout int) error {
	cont, err := s.GetContest(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	circleID := cont.CircleID

	// Load circle to check user membership and clout balance
	circ, err := s.circleRepo.FindByID(ctx, circleID)
	if err != nil {
		return fmt.Errorf("failed to get circle: %w", err)
	}

	userMember, exists := circ.Members[userID]
	if !exists {
		return ErrUserNotInCircle
	}

	existingStake := 0
	for _, pred := range cont.Predictions {
		if pred.UserID == userID && pred.OptionID == optionID {
			existingStake = pred.Clout
			break
		}
	}

	delta := clout - existingStake
	if delta > 0 && userMember.Clout < delta {
		return ErrInsufficientClout
	}

	// Record the prediction in the contest
	err = s.RecordPrediction(ctx, contestID, userID, optionID, clout)
	if err != nil {
		return fmt.Errorf("failed to record prediction: %w", err)
	}

	// Adjust clout balance by the delta between old and new stakes
	newClout := userMember.Clout - delta
	err = s.circleRepo.UpdateMemberClout(ctx, circleID, int32(userID), newClout)
	if err != nil {
		return fmt.Errorf("failed to update member clout: %w", err)
	}

	return nil
}

// ResolveAndDistributeContestClout resolves a contest and distributes winnings to the circle members.
func (s *Service) ResolveAndDistributeContestClout(ctx context.Context, contestID contest.ID, resolverID user.ID, winningOptionID int) error {
	cont, err := s.GetContest(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	circleID := cont.CircleID

	// Resolve the contest
	err = s.ResolveContest(ctx, contestID, resolverID, winningOptionID)
	if err != nil {
		return fmt.Errorf("failed to resolve contest: %w", err)
	}

	// Calculate payouts
	payouts, err := s.CalculatePayouts(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to calculate payouts: %w", err)
	}

	// Load circle to update member clout
	circ, err := s.circleRepo.FindByID(ctx, circleID)
	if err != nil {
		return fmt.Errorf("failed to get circle: %w", err)
	}

	// Update clout for each winner
	for winnerID, payout := range payouts {
		if member, exists := circ.Members[winnerID]; exists {
			newClout := member.Clout + payout
			err := s.circleRepo.UpdateMemberClout(ctx, circleID, int32(winnerID), newClout)
			if err != nil {
				return fmt.Errorf("failed to update winner clout for user %d: %w", winnerID, err)
			}
		}
	}

	return nil
}

// CloseAndRefundContestClout closes a contest without resolution and refunds all staked clout to the circle members.
func (s *Service) CloseAndRefundContestClout(ctx context.Context, contestID contest.ID, closerID user.ID) error {
	cont, err := s.GetContest(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	circleID := cont.CircleID

	// Close the contest and get refunds
	refunds, err := s.CloseContestAndCalculateRefunds(ctx, contestID, closerID)
	if err != nil {
		return fmt.Errorf("failed to close contest: %w", err)
	}

	// Load circle to update member clout
	circ, err := s.circleRepo.FindByID(ctx, circleID)
	if err != nil {
		return fmt.Errorf("failed to get circle: %w", err)
	}

	// Update clout for each predictor being refunded
	for userID, refundAmount := range refunds {
		if member, exists := circ.Members[userID]; exists {
			newClout := member.Clout + refundAmount
			err := s.circleRepo.UpdateMemberClout(ctx, circleID, int32(userID), newClout)
			if err != nil {
				return fmt.Errorf("failed to update refunded clout for user %d: %w", userID, err)
			}
		}
	}

	return nil
}

// Contest operations

// CreateContest creates a new contest in a circle.
func (s *Service) CreateContest(ctx context.Context, circleID circle.ID, creatorID user.ID, question string, options []string, duration string, minStake int) (*contest.Contest, error) {
	// Create domain entity (validation happens here)
	c, err := contest.New(s.clock, circleID, creatorID, question, options, duration, minStake)
	if err != nil {
		return nil, err
	}

	// Persist using repository
	err = s.contestRepo.Save(ctx, c)
	if err != nil {
		return nil, err
	}

	return c, nil
}

// RecordPrediction adds a prediction to a contest and persists it.
func (s *Service) RecordPrediction(ctx context.Context, contestID contest.ID, userID user.ID, optionID int, clout int) error {
	// Load contest from repository
	c, err := s.contestRepo.FindByID(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	// Use domain method to add prediction
	err = c.Predict(userID, optionID, clout)
	if err != nil {
		return err
	}

	// Save updated contest
	err = s.contestRepo.Save(ctx, c)
	if err != nil {
		return fmt.Errorf("failed to save prediction: %w", err)
	}

	return nil
}

// LockContest transitions a contest from Open to Locked.
func (s *Service) LockContest(ctx context.Context, contestID contest.ID, lockerID user.ID) error {
	// Load contest from repository
	c, err := s.contestRepo.FindByID(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	if c.CreatorID != lockerID {
		return ErrNotContestCreator
	}

	// Use domain method to lock
	err = c.Lock()
	if err != nil {
		return err
	}

	// Save updated contest
	err = s.contestRepo.Save(ctx, c)
	if err != nil {
		return fmt.Errorf("failed to save locked contest: %w", err)
	}

	return nil
}

// ResolveContest marks a contest as resolved with a winning option.
func (s *Service) ResolveContest(ctx context.Context, contestID contest.ID, resolverID user.ID, winningOptionID int) error {
	// Load contest from repository
	c, err := s.contestRepo.FindByID(ctx, contestID)
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
	err = s.contestRepo.Save(ctx, c)
	if err != nil {
		return fmt.Errorf("failed to save resolved contest: %w", err)
	}

	return nil
}

// CalculatePayouts calculates winner payouts for a resolved contest.
func (s *Service) CalculatePayouts(ctx context.Context, contestID contest.ID) (map[user.ID]int, error) {
	// Load contest from repository
	c, err := s.contestRepo.FindByID(ctx, contestID)
	if err != nil {
		return nil, fmt.Errorf("failed to get contest: %w", err)
	}

	// Calculate payouts using domain method
	payouts, err := c.CalculateWinnerPayouts()
	if err != nil {
		return nil, err
	}

	return payouts, nil
}

// CloseContestAndCalculateRefunds closes a contest without resolution and returns refunds.
func (s *Service) CloseContestAndCalculateRefunds(ctx context.Context, contestID contest.ID, closerID user.ID) (map[user.ID]int, error) {
	// Load contest from repository
	c, err := s.contestRepo.FindByID(ctx, contestID)
	if err != nil {
		return nil, fmt.Errorf("failed to get contest: %w", err)
	}

	if c.CreatorID != closerID {
		return nil, ErrNotContestCreator
	}

	// Use domain method to close
	err = c.Close()
	if err != nil {
		return nil, err
	}

	// Calculate refunds for all predictors (100% of their stake)
	refunds := s.calculateRefunds(c)

	// Save updated contest
	err = s.contestRepo.Save(ctx, c)
	if err != nil {
		return nil, fmt.Errorf("failed to save closed contest: %w", err)
	}

	return refunds, nil
}

// calculateRefunds returns all predictions as full refunds (100% of each prediction).
func (s *Service) calculateRefunds(c *contest.Contest) map[user.ID]int {
	refunds := make(map[user.ID]int)

	// Sum all predictions by user (in case user has multiple predictions on different options)
	for _, pred := range c.Predictions {
		refunds[pred.UserID] += pred.Clout
	}

	return refunds
}

// GetContest retrieves a contest by ID.
func (s *Service) GetContest(ctx context.Context, id contest.ID) (*contest.Contest, error) {
	return s.contestRepo.FindByID(ctx, id)
}

// GetContestsByCircleID retrieves all contests in a circle.
func (s *Service) GetContestsByCircleID(ctx context.Context, circleID circle.ID) ([]*contest.Contest, error) {
	return s.contestRepo.FindByCircleID(ctx, circleID)
}
