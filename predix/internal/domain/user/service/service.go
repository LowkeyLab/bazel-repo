package service

import (
	"context"
	"errors"
	"strings"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"golang.org/x/crypto/bcrypt"
)

// Service provides user-related operations.
type Service struct {
	repo repository.Repository
}

// NewService constructs a Service.
func NewService(repo repository.Repository) *Service {
	return &Service{repo: repo}
}

// Login finds an existing user by username and verifies password.
func (s *Service) Login(ctx context.Context, username, password string) (*user.User, error) {
	username = strings.TrimSpace(username)
	password = strings.TrimSpace(password)

	if username == "" || password == "" {
		return nil, errors.New("username and password are required")
	}

	existing, err := s.repo.FindByUsername(ctx, username)
	if err != nil {
		if errors.Is(err, repository.ErrNotFound) {
			return nil, errors.New("invalid credentials")
		}
		return nil, err
	}

	if bcrypt.CompareHashAndPassword([]byte(existing.PasswordHash), []byte(password)) != nil {
		return nil, errors.New("invalid credentials")
	}

	return existing, nil
}

// Register creates a brand-new user account. It returns an error if the
// username is already taken.
func (s *Service) Register(ctx context.Context, username, password string) (*user.User, error) {
	username = strings.TrimSpace(username)
	password = strings.TrimSpace(password)

	if username == "" || password == "" {
		return nil, errors.New("username and password are required")
	}

	if _, err := s.repo.FindByUsername(ctx, username); err == nil {
		return nil, errors.New("username already exists")
	} else if !errors.Is(err, repository.ErrNotFound) {
		return nil, err
	}

	return s.createUser(ctx, username, password)
}

func (s *Service) createUser(ctx context.Context, username, password string) (*user.User, error) {
	hash, err := bcrypt.GenerateFromPassword([]byte(password), bcrypt.DefaultCost)
	if err != nil {
		return nil, err
	}

	newUser, err := user.New(username, string(hash))
	if err != nil {
		return nil, err
	}

	if err := s.repo.Save(ctx, newUser); err != nil {
		return nil, err
	}

	return newUser, nil
}
