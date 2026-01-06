package user_test

import (
	"testing"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

func TestNew(t *testing.T) {
	username := "alice"
	passwordHash := "hash"

	u, err := user.New(username, passwordHash)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if u.Username != username {
		t.Errorf("expected username %q, got %q", username, u.Username)
	}
	if u.PasswordHash != passwordHash {
		t.Errorf("expected password hash %q, got %q", passwordHash, u.PasswordHash)
	}
	if u.ID != 0 {
		t.Errorf("expected ID to be 0 (unassigned), got %d", u.ID)
	}
	if u.Role != user.RoleMember {
		t.Errorf("expected role to default to %q, got %q", user.RoleMember, u.Role)
	}
}
