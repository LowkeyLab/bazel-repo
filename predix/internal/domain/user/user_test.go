package user_test

import (
	"testing"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

func TestNew(t *testing.T) {
	username := "alice"
	authorizerID := "auth-id"

	u, err := user.New(username, authorizerID)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if u.Username != username {
		t.Errorf("expected username %q, got %q", username, u.Username)
	}
	if u.ID != user.ID(authorizerID) {
		t.Errorf("expected ID %q, got %q", authorizerID, u.ID)
	}
}
