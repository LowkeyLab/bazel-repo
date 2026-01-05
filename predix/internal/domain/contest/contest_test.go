package contest_test

import (
	"testing"
	"time"

	clockpkg "github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNew(t *testing.T) {
	circleID := circle.ID(1)
	creatorID := user.ID(1)
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}
	options := []string{"Yes", "No"}

	c, err := contest.New(clk, circleID, creatorID, "Will it rain?", options, contest.Duration1Hour, 10)
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
	circleID := circle.ID(1)
	creatorID := user.ID(1)
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}

	tests := []struct {
		name     string
		question string
		options  []string
		minStake int
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
		{
			name:     "invalid min stake",
			question: "Valid question?",
			options:  []string{"Yes", "No"},
			minStake: 5,
			wantErr:  "min stake must be one of 10, 100, or 1000",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c, err := contest.New(clk, circleID, creatorID, tt.question, tt.options, contest.Duration1Hour, tt.minStake)
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
	circleID := circle.ID(1)
	creatorID := user.ID(1)
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}

	tests := []struct {
		name     string
		question string
		options  []string
		minStake int
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
		{
			name:     "explicit min stake 100",
			question: "Pick one",
			options:  []string{"Option One", "Option Two"},
			minStake: 100,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c, err := contest.New(clk, circleID, creatorID, tt.question, tt.options, contest.Duration1Hour, tt.minStake)
			if err != nil {
				t.Errorf("unexpected error: %v", err)
			}
			if c == nil {
				t.Fatal("expected non-nil contest")
			}
			if tt.minStake == 0 && c.MinStake != 10 {
				t.Errorf("expected default min stake 10, got %d", c.MinStake)
			}
			if tt.minStake != 0 && c.MinStake != tt.minStake {
				t.Errorf("expected min stake %d, got %d", tt.minStake, c.MinStake)
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
	circleID := circle.ID(1)
	creatorID := user.ID(1)
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}
	c, err := contest.New(clk, circleID, creatorID, "Q?", []string{"A", "B"}, contest.Duration1Hour, 100)
	if err != nil {
		t.Fatalf("unexpected error creating contest: %v", err)
	}

	betterID := user.ID(1)

	// Find valid option ID (it's 1 or 2 based on logic)
	var optionID int
	for id := range c.Options {
		optionID = id
		break
	}

	err = c.Predict(betterID, optionID, 100)
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

	err = c.Predict(betterID, optionID, 10)
	if err == nil {
		t.Fatal("expected error for clout below min stake")
	}
}

func TestPredict_UpdateExistingStake(t *testing.T) {
	circleID := circle.ID(2)
	creatorID := user.ID(3)
	clk := clockpkg.FixedClock{Time: time.Now()}
	c, err := contest.New(clk, circleID, creatorID, "Q?", []string{"A", "B"}, contest.Duration1Hour, 100)
	require.NoError(t, err)

	betterID := user.ID(4)
	optionID := 1

	require.NoError(t, c.Predict(betterID, optionID, 200))
	require.Len(t, c.Predictions, 1)
	require.Equal(t, 200, c.Predictions[0].Clout)

	err = c.Predict(betterID, optionID, 120)
	require.NoError(t, err)
	assert.Len(t, c.Predictions, 1, "should update in place instead of appending")
	assert.Equal(t, 120, c.Predictions[0].Clout)

	// Updating below the contest min stake is rejected and keeps the previous value
	err = c.Predict(betterID, optionID, 50)
	assert.Error(t, err)
	assert.Equal(t, 120, c.Predictions[0].Clout)
}

func TestResolve(t *testing.T) {
	circleID := circle.ID(1)
	creatorID := user.ID(1)
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}
	c, _ := contest.New(clk, circleID, creatorID, "Q?", []string{"A", "B"}, contest.Duration1Hour, 10)

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

func TestClose(t *testing.T) {
	circleID := circle.ID(1)
	creatorID := user.ID(1)
	now := time.Now()
	clk := clockpkg.FixedClock{Time: now}

	tests := []struct {
		name       string
		setup      func(*contest.Contest)
		wantErr    bool
		wantErrMsg string
	}{
		{
			name:    "close open contest",
			setup:   func(c *contest.Contest) {},
			wantErr: false,
		},
		{
			name: "close locked contest",
			setup: func(c *contest.Contest) {
				c.Lock()
			},
			wantErr: false,
		},
		{
			name: "cannot close resolved contest",
			setup: func(c *contest.Contest) {
				var optionID int
				for id := range c.Options {
					optionID = id
					break
				}
				c.Resolve(optionID)
			},
			wantErr:    true,
			wantErrMsg: "cannot close a resolved contest",
		},
		{
			name: "cannot close already closed contest",
			setup: func(c *contest.Contest) {
				c.Close()
			},
			wantErr:    true,
			wantErrMsg: "contest is already closed",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c, _ := contest.New(clk, circleID, creatorID, "Q?", []string{"A", "B"}, contest.Duration1Hour, 10)
			tt.setup(c)

			err := c.Close()
			if tt.wantErr {
				if err == nil {
					t.Errorf("expected error, got nil")
				} else if tt.wantErrMsg != "" && err.Error() != tt.wantErrMsg {
					t.Errorf("expected error %q, got %q", tt.wantErrMsg, err.Error())
				}
			} else {
				if err != nil {
					t.Errorf("unexpected error: %v", err)
				}
				if c.Status != contest.StatusClosed {
					t.Errorf("expected status Closed, got %s", c.Status)
				}
			}
		})
	}
}
