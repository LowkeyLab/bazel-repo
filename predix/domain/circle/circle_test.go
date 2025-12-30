package circle_test

import (
	"testing"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/domain/user"
)

func TestNew(t *testing.T) {
	creatorID := user.ID(uuid.New())
	name := "Test Circle"

	c, err := circle.New(name, creatorID)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if c.Name != name {
		t.Errorf("expected name %q, got %q", name, c.Name)
	}
	if c.ID.String() == "" {
		t.Error("expected valid ID")
	}
	if len(c.Members) != 1 {
		t.Errorf("expected 1 member, got %d", len(c.Members))
	}

	member, ok := c.Members[creatorID]
	if !ok {
		t.Fatal("creator not found in members")
	}
	if member.UserID != creatorID {
		t.Errorf("expected member ID %s, got %s", creatorID, member.UserID)
	}
}

func TestNew_EmptyName(t *testing.T) {
	creatorID := user.ID(uuid.New())
	_, err := circle.New("", creatorID)
	if err == nil {
		t.Error("expected error for empty name, got nil")
	}
}

func TestAddMember(t *testing.T) {
	creatorID := user.ID(uuid.New())
	c, _ := circle.New("Test Circle", creatorID)

	newUserID := user.ID(uuid.New())
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
