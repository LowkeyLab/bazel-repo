package application

import (
	"context"
	"time"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/domain/prediction"
	"github.com/lowkeylab/bazel-repo/predix/domain/user"
)

type Service struct {
	users       user.Repository
	circles     circle.Repository
	predictions prediction.Repository
}

func NewService(u user.Repository, c circle.Repository, p prediction.Repository) *Service {
	return &Service{
		users:       u,
		circles:     c,
		predictions: p,
	}
}

func (s *Service) CreateUser(ctx context.Context, name, email string) (*user.User, error) {
	u, err := user.New(name, email)
	if err != nil {
		return nil, err
	}
	if err := s.users.Save(ctx, u); err != nil {
		return nil, err
	}
	return u, nil
}

func (s *Service) CreateCircle(ctx context.Context, name, creatorID string) (*circle.Circle, error) {
	// Verify user exists
	creatorUUID, err := parseUUID(creatorID)
	if err != nil {
		return nil, err
	}
	uID := user.ID(creatorUUID)
	if _, err := s.users.FindByID(ctx, uID); err != nil {
		return nil, err
	}

	c, err := circle.New(name, uID)
	if err != nil {
		return nil, err
	}
	if err := s.circles.Save(ctx, c); err != nil {
		return nil, err
	}
	return c, nil
}

func (s *Service) JoinCircle(ctx context.Context, userID, inviteCode string) error {
	userUUID, err := parseUUID(userID)
	if err != nil {
		return err
	}

	c, err := s.circles.FindByInviteCode(ctx, inviteCode)
	if err != nil {
		return err
	}

	c.AddMember(user.ID(userUUID))
	return s.circles.Save(ctx, c)
}

func (s *Service) CreatePrediction(ctx context.Context, circleIDStr, creatorID, question string, options []string, expiresAt time.Time) (*prediction.Prediction, error) {
	circleUUID, err := parseUUID(circleIDStr)
	if err != nil {
		return nil, err
	}
	
	creatorUUID, err := parseUUID(creatorID)
	if err != nil {
		return nil, err
	}

	p, err := prediction.New(circle.ID(circleUUID), user.ID(creatorUUID), question, options, expiresAt)
	if err != nil {
		return nil, err
	}
	if err := s.predictions.Save(ctx, p); err != nil {
		return nil, err
	}
	return p, nil
}

func (s *Service) PlaceBet(ctx context.Context, predictionIDStr, userID, optionID string, amount int) error {
	predUUID, err := parseUUID(predictionIDStr)
	if err != nil {
		return err
	}
	
	userUUID, err := parseUUID(userID)
	if err != nil {
		return err
	}

	p, err := s.predictions.FindByID(ctx, prediction.ID(predUUID))
	if err != nil {
		return err
	}

	if err := p.PlaceBet(user.ID(userUUID), optionID, amount); err != nil {
		return err
	}

	// Update user balance in circle (omitted for brevity, requires transaction/coordination)
	// For MVP, we assume infinite credit or separate validation

	return s.predictions.Save(ctx, p)
}
func (s *Service) ResolvePrediction(ctx context.Context, predictionIDStr, winningOptionID string) error {
	predUUID, err := parseUUID(predictionIDStr)
	if err != nil {
		return err
	}

	p, err := s.predictions.FindByID(ctx, prediction.ID(predUUID))
	if err != nil {
		return err
	}

	if err := p.Resolve(winningOptionID); err != nil {
		return err
	}

	// Calculate payouts (omitted for brevity)

	return s.predictions.Save(ctx, p)
}

func parseUUID(s string) (uuid.UUID, error) {
	// Placeholder for actual UUID parsing if we were using the google/uuid package directly here
	// But our domain uses typed IDs.
	// Since the input is string, we should probably just return the ID if valid.
	// The google/uuid package is imported in domain, let's just cast for now or rely on string parsing in a real app.
	// For this exercise, I'll assume valid UUID strings.
	// Wait, I need to actually import uuid to parse it.
	return uuid.Parse(s)
}
