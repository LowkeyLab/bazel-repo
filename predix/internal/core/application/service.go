package application

import (
	"context"
	"time"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/domain/user"
)

type Service struct {
	users    user.Repository
	circles  circle.Repository
	contests contest.Repository
}

func NewService(u user.Repository, c circle.Repository, co contest.Repository) *Service {
	return &Service{
		users:    u,
		circles:  c,
		contests: co,
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

func (s *Service) CreateContest(ctx context.Context, circleIDStr, creatorID, question string, options []string, expiresAt time.Time) (*contest.Contest, error) {
	circleUUID, err := parseUUID(circleIDStr)
	if err != nil {
		return nil, err
	}

	creatorUUID, err := parseUUID(creatorID)
	if err != nil {
		return nil, err
	}

	c, err := contest.New(circle.ID(circleUUID), user.ID(creatorUUID), question, options, expiresAt)
	if err != nil {
		return nil, err
	}
	if err := s.contests.Save(ctx, c); err != nil {
		return nil, err
	}
	return c, nil
}

func (s *Service) Predict(ctx context.Context, contestIDStr, userID string, optionID int, clout int) error {
	contestUUID, err := parseUUID(contestIDStr)
	if err != nil {
		return err
	}

	userUUID, err := parseUUID(userID)
	if err != nil {
		return err
	}

	c, err := s.contests.FindByID(ctx, contest.ID(contestUUID))
	if err != nil {
		return err
	}

	if err := c.Predict(user.ID(userUUID), optionID, clout); err != nil {
		return err
	}
	// Update user balance in circle (omitted for brevity, requires transaction/coordination)

	return s.contests.Save(ctx, c)
}

func (s *Service) ResolveContest(ctx context.Context, contestIDStr string, winningOptionID int) error {
	contestUUID, err := parseUUID(contestIDStr)
	if err != nil {
		return err
	}

	c, err := s.contests.FindByID(ctx, contest.ID(contestUUID))
	if err != nil {
		return err
	}

	if err := c.Resolve(winningOptionID); err != nil {
		return err
	}

	// Calculate payouts (omitted for brevity)

	return s.contests.Save(ctx, c)
}

func parseUUID(s string) (uuid.UUID, error) {
	return uuid.Parse(s)
}
