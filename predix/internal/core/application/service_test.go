package application_test

import (
	"context"
	"testing"

	"github.com/lowkeylab/bazel-repo/predix/internal/core/application"
	"github.com/lowkeylab/bazel-repo/predix/internal/infrastructure/inmemory"
)

func TestCreateCircle(t *testing.T) {
	ctx := context.Background()
	userRepo := inmemory.NewUserRepository()
	circleRepo := inmemory.NewCircleRepository()
	contestRepo := inmemory.NewContestRepository()
	svc := application.NewService(userRepo, circleRepo, contestRepo)

	// Create user first
	u, err := svc.CreateUser(ctx, "Test User", "test@example.com")
	if err != nil {
		t.Fatalf("failed to create user: %v", err)
	}

	// Create circle
	c, err := svc.CreateCircle(ctx, "Test Circle", u.ID.String())
	if err != nil {
		t.Fatalf("failed to create circle: %v", err)
	}

	if c.Name != "Test Circle" {
		t.Errorf("expected circle name 'Test Circle', got '%s'", c.Name)
	}

	if _, ok := c.Members[u.ID]; !ok {
		t.Error("creator should be a member of the circle")
	}
}
