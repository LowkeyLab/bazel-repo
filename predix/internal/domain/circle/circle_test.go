package circle_test

import (
	"testing"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

func TestNew(t *testing.T) {
	creatorID := user.ID(1)
	name := "Test Circle"

	c, err := circle.New(name, creatorID)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if c.Name != name {
		t.Errorf("expected name %q, got %q", name, c.Name)
	}
	if c.ID != 0 {
		t.Errorf("expected ID to be 0 (unassigned), got %d", c.ID)
	}
	if len(c.Members) != 1 {
		t.Errorf("expected 1 member, got %d", len(c.Members))
	}

	member, ok := c.Members[creatorID]
	if !ok {
		t.Fatal("creator not found in members")
	}
	if member.UserID != creatorID {
		t.Errorf("expected member ID %d, got %d", creatorID, member.UserID)
	}
}

func TestNew_EmptyName(t *testing.T) {
	creatorID := user.ID(1)
	c, err := circle.New("", creatorID)
	if err == nil {
		t.Error("expected error for empty name, got nil")
	}
	if c != nil {
		t.Error("expected nil circle on error")
	}
	expectedErr := "circle name cannot be empty"
	if err.Error() != expectedErr {
		t.Errorf("expected error %q, got %q", expectedErr, err.Error())
	}
}

func TestNew_ValidNames(t *testing.T) {
	creatorID := user.ID(1)

	tests := []struct {
		name       string
		circleName string
	}{
		{
			name:       "simple name",
			circleName: "Book Club",
		},
		{
			name:       "name with numbers",
			circleName: "Team 42",
		},
		{
			name:       "name with special chars",
			circleName: "Friends & Family",
		},
		{
			name:       "long name",
			circleName: "The Super Awesome Amazing Friends Circle",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c, err := circle.New(tt.circleName, creatorID)
			if err != nil {
				t.Errorf("unexpected error: %v", err)
			}
			if c == nil {
				t.Fatal("expected non-nil circle")
			}
			if c.Name != tt.circleName {
				t.Errorf("expected name %q, got %q", tt.circleName, c.Name)
			}
			if len(c.InviteCode) == 0 {
				t.Error("expected non-empty invite code")
			}
		})
	}
}

func TestAddMember(t *testing.T) {
	creatorID := user.ID(1)
	c, _ := circle.New("Test Circle", creatorID)

	newUserID := user.ID(2)
	c.AddMember(newUserID)

	if len(c.Members) != 2 {
		t.Errorf("expected 2 members, got %d", len(c.Members))
	}

	member, ok := c.Members[newUserID]
	if !ok {
		t.Fatal("new member not found")
	}
	if member.Clout != 1000 {
		t.Errorf("expected initial clout 1000, got %d", member.Clout)
	}

	// Test adding existing member (should be idempotent / no-op for existing)
	c.AddMember(newUserID)
	if len(c.Members) != 2 {
		t.Errorf("expected 2 members after re-adding, got %d", len(c.Members))
	}
}
