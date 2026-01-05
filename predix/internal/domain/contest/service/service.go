package service

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

type ContestRepository = repository.Repository

var ErrNotContestCreator = errors.New("only the contest creator can resolve this contest")

type Service struct {
	repo repository.Repository
}

// NewService creates a service with repository interfaces.
func NewService(repo ContestRepository) *Service {
	return &Service{
		repo: repo,
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
