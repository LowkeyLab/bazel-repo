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

// TransactionManager defines an interface for executing code within a transaction.
type TransactionManager interface {
	RunInTx(ctx context.Context, fn func(ctx context.Context) error) error
}

type Service struct {
	circleRepo  repository.Repository
	userRepo    userrepo.Repository
	contestRepo contestrepo.Repository
	clock       clock.Clock
	txm         TransactionManager
}

// NewService creates a service with a repository interface.
func NewService(circleRepo CircleRepository, userRepo userrepo.Repository, contestRepo ContestRepository, clk clock.Clock, txm TransactionManager) *Service {
	return &Service{
		circleRepo:  circleRepo,
		userRepo:    userRepo,
		contestRepo: contestRepo,
		clock:       clk,
		txm:         txm,
	}
}

var (
	ErrNotCircleOwner    = errors.New("only the circle creator or an admin can delete this circle")
	ErrInsufficientClout = errors.New("insufficient clout balance to make this prediction")
	ErrUserNotInCircle   = errors.New("user is not a member of this circle")
	ErrNotContestCreator = errors.New("only the contest creator can resolve this contest")
	ErrContestNotFound   = errors.New("contest not found or does not belong to this circle")
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

type PayoutRecord struct {
	UserID   int32
	Username string
	Stake    int
	Share    int
	Total    int
}

type PayoutBreakdown struct {
	Winners          []PayoutRecord
	Losers           []PayoutRecord
	TotalPot         int
	HouseRake        int
	DistributablePot int
	TotalDistributed int
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
func (s *Service) Predict(ctx context.Context, circleID circle.ID, contestID contest.ID, userID user.ID, optionID int, clout int) error {
	cont, err := s.GetContest(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	if cont.CircleID != circleID {
		return ErrContestNotFound
	}

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
func (s *Service) ResolveAndDistributeContestClout(ctx context.Context, circleID circle.ID, contestID contest.ID, resolverID user.ID, winningOptionID int) error {
	cont, err := s.GetContest(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	if cont.CircleID != circleID {
		return ErrContestNotFound
	}

	// Resolve the contest and get winner payouts
	payouts, err := s.ResolveContestAndCalculatePayouts(ctx, contestID, resolverID, winningOptionID)
	if err != nil {
		return fmt.Errorf("failed to resolve contest: %w", err)
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
func (s *Service) LockContest(ctx context.Context, circleID circle.ID, contestID contest.ID, lockerID user.ID) error {
	// Load contest from repository
	c, err := s.contestRepo.FindByID(ctx, contestID)
	if err != nil {
		return fmt.Errorf("failed to get contest: %w", err)
	}

	if c.CircleID != circleID {
		return ErrContestNotFound
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

// ResolveContestAndCalculatePayouts resolves a contest and returns winner payouts.
func (s *Service) ResolveContestAndCalculatePayouts(ctx context.Context, contestID contest.ID, resolverID user.ID, winningOptionID int) (map[user.ID]int, error) {
	// Load contest from repository
	c, err := s.contestRepo.FindByID(ctx, contestID)
	if err != nil {
		return nil, fmt.Errorf("failed to get contest: %w", err)
	}

	if c.CreatorID != resolverID {
		return nil, ErrNotContestCreator
	}

	// Use domain method to resolve
	err = c.Resolve(winningOptionID)
	if err != nil {
		return nil, err
	}

	// Calculate payouts using domain method
	payouts, err := c.CalculateWinnerPayouts()
	if err != nil {
		return nil, err
	}

	// Save updated contest
	err = s.contestRepo.Save(ctx, c)
	if err != nil {
		return nil, fmt.Errorf("failed to save resolved contest: %w", err)
	}

	return payouts, nil
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

// ExpireContest closes an expired contest and refunds all staked clout.
// This is intended for system use and does not require a user initiator.
// The entire operation is wrapped in a transaction to ensure atomicity.
func (s *Service) ExpireContest(ctx context.Context, contestID contest.ID) error {
	return s.txm.RunInTx(ctx, func(ctx context.Context) error {
		cont, err := s.GetContest(ctx, contestID)
		if err != nil {
			return fmt.Errorf("failed to get contest: %w", err)
		}

		circleID := cont.CircleID

		// Use domain method to close (validates state)
		if err := cont.Close(); err != nil {
			return err
		}

		// Calculate refunds for all predictors (100% of their stake)
		refunds := s.calculateRefunds(cont)

		// Save updated contest
		if err := s.contestRepo.Save(ctx, cont); err != nil {
			return fmt.Errorf("failed to save expired contest: %w", err)
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
				if err := s.circleRepo.UpdateMemberClout(ctx, circleID, int32(userID), newClout); err != nil {
					return fmt.Errorf("failed to update refunded clout for user %d: %w", userID, err)
				}
			}
		}

		return nil
	})
}

// GetContest retrieves a contest by ID.
func (s *Service) GetContest(ctx context.Context, id contest.ID) (*contest.Contest, error) {
	return s.contestRepo.FindByID(ctx, id)
}

// GetContestInCircle retrieves a contest and verifies it belongs to the circle.
func (s *Service) GetContestInCircle(ctx context.Context, circleID circle.ID, contestID contest.ID) (*contest.Contest, error) {
	c, err := s.contestRepo.FindByID(ctx, contestID)
	if err != nil {
		return nil, err
	}

	if c.CircleID != circleID {
		return nil, ErrContestNotFound
	}

	return c, nil
}

// GetPayoutBreakdown calculates the payout breakdown for a resolved contest.
func (s *Service) GetPayoutBreakdown(ctx context.Context, circleID circle.ID, contestID contest.ID) (*PayoutBreakdown, error) {
	c, err := s.GetContestInCircle(ctx, circleID, contestID)
	if err != nil {
		return nil, err
	}

	if c.Status != contest.StatusResolved {
		return nil, fmt.Errorf("contest must be resolved to view payout breakdown")
	}

	winners, losers, err := c.CalculatePayoutBreakdown()
	if err != nil {
		return nil, err
	}

	winnerRecords := make([]PayoutRecord, len(winners))
	for i, r := range winners {
		username := "Unknown"
		u, err := s.userRepo.FindByID(ctx, r.UserID)
		if err == nil {
			username = u.Username
		}
		winnerRecords[i] = PayoutRecord{
			UserID:   int32(r.UserID),
			Username: username,
			Stake:    r.OriginalStake,
			Share:    r.ShareOfPot,
			Total:    r.TotalPayout,
		}
	}

	loserRecords := make([]PayoutRecord, len(losers))
	for i, r := range losers {
		username := "Unknown"
		u, err := s.userRepo.FindByID(ctx, r.UserID)
		if err == nil {
			username = u.Username
		}
		loserRecords[i] = PayoutRecord{
			UserID:   int32(r.UserID),
			Username: username,
			Stake:    r.OriginalStake,
			Share:    r.ShareOfPot,
			Total:    r.TotalPayout,
		}
	}

	totalPot := 0
	winningClout := 0
	for _, p := range c.Predictions {
		totalPot += p.Clout
		if c.ResultOptionID != nil && p.OptionID == *c.ResultOptionID {
			winningClout += p.Clout
		}
	}
	losingClout := totalPot - winningClout
	houseRake := int(float64(losingClout) * c.HouseRake)

	totalDistributed := 0
	for _, r := range winnerRecords {
		totalDistributed += r.Total
	}

	return &PayoutBreakdown{
		Winners:          winnerRecords,
		Losers:           loserRecords,
		TotalPot:         totalPot,
		HouseRake:        houseRake,
		DistributablePot: totalPot - houseRake,
		TotalDistributed: totalDistributed,
	}, nil
}

// GetContestsByCircleID retrieves all contests in a circle.
func (s *Service) GetContestsByCircleID(ctx context.Context, circleID circle.ID) ([]*contest.Contest, error) {
	return s.contestRepo.FindByCircleID(ctx, circleID)
}
