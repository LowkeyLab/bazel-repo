package contest_test

import (
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

func TestNew(t *testing.T) {
	circleID := circle.ID(uuid.New())
	creatorID := user.ID(uuid.New())
	expiresAt := time.Now().Add(1 * time.Hour)
	options := []string{"Yes", "No"}

	c, err := contest.New(circleID, creatorID, "Will it rain?", options, expiresAt)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if c.Status != contest.StatusOpen {
		t.Errorf("expected status Open, got %s", c.Status)
	}
	if len(c.Options) != 2 {
		t.Errorf("expected 2 options, got %d", len(c.Options))
	}
}

func TestNew_Validation(t *testing.T) {
	circleID := circle.ID(uuid.New())
	creatorID := user.ID(uuid.New())
	expiresAt := time.Now().Add(1 * time.Hour)

	tests := []struct {
		name     string
		question string
		options  []string
		wantErr  string
	}{
		{
			name:     "less than 2 options",
			question: "Valid question?",
			options:  []string{"One"},
			wantErr:  "at least two options are required",
		},
		{
			name:     "empty question",
			question: "",
			options:  []string{"Yes", "No"},
			wantErr:  "contest question cannot be empty",
		},
		{
			name:     "empty option text",
			question: "Valid question?",
			options:  []string{"Yes", ""},
			wantErr:  "option text cannot be empty",
		},
		{
			name:     "duplicate options",
			question: "Valid question?",
			options:  []string{"Yes", "No", "Yes"},
			wantErr:  "duplicate options are not allowed",
		},
		{
			name:     "all empty options",
			question: "Valid question?",
			options:  []string{"", ""},
			wantErr:  "option text cannot be empty",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c, err := contest.New(circleID, creatorID, tt.question, tt.options, expiresAt)
			if err == nil {
				t.Errorf("expected error containing %q, got nil", tt.wantErr)
			} else if err.Error() != tt.wantErr {
				t.Errorf("expected error %q, got %q", tt.wantErr, err.Error())
			}
			if c != nil {
				t.Error("expected nil contest on error")
			}
		})
	}
}

func TestNew_ValidContest(t *testing.T) {
	circleID := circle.ID(uuid.New())
	creatorID := user.ID(uuid.New())
	expiresAt := time.Now().Add(1 * time.Hour)

	tests := []struct {
		name     string
		question string
		options  []string
	}{
		{
			name:     "two options",
			question: "Yes or No?",
			options:  []string{"Yes", "No"},
		},
		{
			name:     "three options",
			question: "Which one?",
			options:  []string{"A", "B", "C"},
		},
		{
			name:     "options with spaces",
			question: "What's your favorite?",
			options:  []string{"Option One", "Option Two", "Option Three"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c, err := contest.New(circleID, creatorID, tt.question, tt.options, expiresAt)
			if err != nil {
				t.Errorf("unexpected error: %v", err)
			}
			if c == nil {
				t.Fatal("expected non-nil contest")
			}
			if c.Question != tt.question {
				t.Errorf("expected question %q, got %q", tt.question, c.Question)
			}
			if len(c.Options) != len(tt.options) {
				t.Errorf("expected %d options, got %d", len(tt.options), len(c.Options))
			}
		})
	}
}

func TestPredict(t *testing.T) {
	circleID := circle.ID(uuid.New())
	creatorID := user.ID(uuid.New())
	expiresAt := time.Now().Add(1 * time.Hour)
	c, _ := contest.New(circleID, creatorID, "Q?", []string{"A", "B"}, expiresAt)

	betterID := user.ID(uuid.New())

	// Find valid option ID (it's 1 or 2 based on logic)
	var optionID int
	for id := range c.Options {
		optionID = id
		break
	}

	err := c.Predict(betterID, optionID, 100)
	if err != nil {
		t.Fatalf("unexpected error making prediction: %v", err)
	}

	if len(c.Predictions) != 1 {
		t.Errorf("expected 1 prediction, got %d", len(c.Predictions))
	}

	// Invalid option
	err = c.Predict(betterID, 999, 100)
	if err == nil {
		t.Error("expected error for invalid option")
	}
}

func TestResolve(t *testing.T) {
	circleID := circle.ID(uuid.New())
	creatorID := user.ID(uuid.New())
	expiresAt := time.Now().Add(1 * time.Hour)
	c, _ := contest.New(circleID, creatorID, "Q?", []string{"A", "B"}, expiresAt)

	// Find valid option ID
	var optionID int
	for id := range c.Options {
		optionID = id
		break
	}

	err := c.Resolve(optionID)
	if err != nil {
		t.Fatalf("unexpected error resolving: %v", err)
	}

	if c.Status != contest.StatusResolved {
		t.Errorf("expected status Resolved, got %s", c.Status)
	}
	if *c.ResultOptionID != optionID {
		t.Errorf("expected result option %d, got %d", optionID, *c.ResultOptionID)
	}

	// Already resolved
	err = c.Resolve(optionID)
	if err == nil {
		t.Error("expected error resolving already resolved contest")
	}
}
