package prediction_test

import (
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/domain/prediction"
	"github.com/lowkeylab/bazel-repo/predix/domain/user"
)

func TestNew(t *testing.T) {
	circleID := circle.ID(uuid.New())
	creatorID := user.ID(uuid.New())
	expiresAt := time.Now().Add(1 * time.Hour)
	options := []string{"Yes", "No"}

	p, err := prediction.New(circleID, creatorID, "Will it rain?", options, expiresAt)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if p.Status != prediction.StatusOpen {
		t.Errorf("expected status Open, got %s", p.Status)
	}
	if len(p.Options) != 2 {
		t.Errorf("expected 2 options, got %d", len(p.Options))
	}
}

func TestNew_Validation(t *testing.T) {
	circleID := circle.ID(uuid.New())
	creatorID := user.ID(uuid.New())
	expiresAt := time.Now().Add(1 * time.Hour)

	_, err := prediction.New(circleID, creatorID, "Q?", []string{"One"}, expiresAt)
	if err == nil {
		t.Error("expected error for less than 2 options")
	}
}

func TestPlaceBet(t *testing.T) {
	circleID := circle.ID(uuid.New())
	creatorID := user.ID(uuid.New())
	expiresAt := time.Now().Add(1 * time.Hour)
	p, _ := prediction.New(circleID, creatorID, "Q?", []string{"A", "B"}, expiresAt)

	betterID := user.ID(uuid.New())

	// Find valid option ID (it's 1 or 2 based on logic)
	var optionID int
	for id := range p.Options {
		optionID = id
		break
	}

	err := p.PlaceBet(betterID, optionID, 100)
	if err != nil {
		t.Fatalf("unexpected error placing bet: %v", err)
	}

	if len(p.Bets) != 1 {
		t.Errorf("expected 1 bet, got %d", len(p.Bets))
	}

	// Invalid option
	err = p.PlaceBet(betterID, 999, 100)
	if err == nil {
		t.Error("expected error for invalid option")
	}
}

func TestResolve(t *testing.T) {
	circleID := circle.ID(uuid.New())
	creatorID := user.ID(uuid.New())
	expiresAt := time.Now().Add(1 * time.Hour)
	p, _ := prediction.New(circleID, creatorID, "Q?", []string{"A", "B"}, expiresAt)

	// Find valid option ID
	var optionID int
	for id := range p.Options {
		optionID = id
		break
	}

	err := p.Resolve(optionID)
	if err != nil {
		t.Fatalf("unexpected error resolving: %v", err)
	}

	if p.Status != prediction.StatusResolved {
		t.Errorf("expected status Resolved, got %s", p.Status)
	}
	if *p.ResultOptionID != optionID {
		t.Errorf("expected result option %d, got %d", optionID, *p.ResultOptionID)
	}

	// Already resolved
	err = p.Resolve(optionID)
	if err == nil {
		t.Error("expected error resolving already resolved prediction")
	}
}
