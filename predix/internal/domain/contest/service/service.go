package service

import (
	"context"
	"errors"
	"fmt"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

type ContestRepository = repository.Repository

var ErrNotContestCreator = errors.New("only the contest creator can resolve this contest")

type Service struct {
	repo  repository.Repository
	clock clock.Clock
}

// NewService creates a service with repository interfaces.
func NewService(repo ContestRepository, clk clock.Clock) *Service {
	return &Service{
		repo:  repo,
		clock: clk,
	}
}

func (s *Service) CreateContest(ctx context.Context, circleID circle.ID, creatorID user.ID, question string, options []string, duration string, minStake int) (*contest.Contest, error) {
	// Create domain entity (validation happens here)
	c, err := contest.New(s.clock, circleID, creatorID, question, options, duration, minStake)
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

// RecordPrediction adds a prediction to a contest and persists it.
// Note: This method does NOT deduct clout; that is handled by the circle service.
func (s *Service) RecordPrediction(ctx context.Context, contestID contest.ID, userID user.ID, optionID int, clout int) error {
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

// LockContest transitions a contest from Open to Locked.
// Only the contest creator can lock the contest.
func (s *Service) LockContest(ctx context.Context, contestID contest.ID, lockerID user.ID) error {
	// Load contest from repository
	c, err := s.repo.FindByID(ctx, contestID)
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
	err = s.repo.Save(ctx, c)
	if err != nil {
		return fmt.Errorf("failed to save locked contest: %w", err)
	}

	return nil
}

// ResolveContestAndCalculatePayouts resolves a contest and returns winner payouts.
// Returns a map of user.ID to clout amount for each winner.
func (s *Service) ResolveContestAndCalculatePayouts(ctx context.Context, contestID contest.ID, resolverID user.ID, winningOptionID int) (map[user.ID]int, error) {
	// Load contest from repository
	c, err := s.repo.FindByID(ctx, contestID)
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

	// Calculate payouts for winners
	payouts := s.calculateWinnerPayouts(c)

	// Save updated contest
	err = s.repo.Save(ctx, c)
	if err != nil {
		return nil, fmt.Errorf("failed to save resolved contest: %w", err)
	}

	return payouts, nil
}

// CloseContestAndCalculateRefunds closes a contest without resolution and returns refunds.
// Only the contest creator can close the contest.
// Returns a map of user.ID to clout amount to refund to each predictor (100% of their stake).
func (s *Service) CloseContestAndCalculateRefunds(ctx context.Context, contestID contest.ID, closerID user.ID) (map[user.ID]int, error) {
	// Load contest from repository
	c, err := s.repo.FindByID(ctx, contestID)
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
	err = s.repo.Save(ctx, c)
	if err != nil {
		return nil, fmt.Errorf("failed to save closed contest: %w", err)
	}

	return refunds, nil
}

// calculateRefunds returns all predictions as full refunds (100% of each prediction).
// Returns a map where keys are user IDs and values are total clout to refund.
func (s *Service) calculateRefunds(c *contest.Contest) map[user.ID]int {
	refunds := make(map[user.ID]int)

	// Sum all predictions by user (in case user has multiple predictions on different options)
	for _, pred := range c.Predictions {
		refunds[pred.UserID] += pred.Clout
	}

	return refunds
}

// calculateWinnerPayouts distributes the remaining pot (after consumption) proportionally to winners.
func (s *Service) calculateWinnerPayouts(c *contest.Contest) map[user.ID]int {
	payouts := make(map[user.ID]int)

	if c.ResultOptionID == nil {
		return payouts
	}

	// Find all predictions that match the winning option
	var winningPredictions []*contest.Prediction
	var totalWinningClout int

	for i := range c.Predictions {
		if c.Predictions[i].OptionID == *c.ResultOptionID {
			winningPredictions = append(winningPredictions, c.Predictions[i])
			totalWinningClout += c.Predictions[i].Clout
		}
	}

	if len(winningPredictions) == 0 {
		return payouts
	}

	// Distribute remaining pot proportionally based on stake
	remainingPot := c.CalculateRemainingPot()
	for _, pred := range winningPredictions {
		// Return original stake
		payout := pred.Clout
		// Add proportional share of remaining pot (after consumption fee)
		if totalWinningClout > 0 {
			payout += (pred.Clout * remainingPot) / totalWinningClout
		}
		payouts[pred.UserID] += payout
	}

	return payouts
}

func (s *Service) GetContest(ctx context.Context, id contest.ID) (*contest.Contest, error) {
	return s.repo.FindByID(ctx, id)
}

func (s *Service) GetContestsByCircleID(ctx context.Context, circleID circle.ID) ([]*contest.Contest, error) {
	return s.repo.FindByCircleID(ctx, circleID)
}
