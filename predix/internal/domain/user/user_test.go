package user_test

import (
	"testing"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

func TestNew(t *testing.T) {
	name := "Alice"
	email := "alice@example.com"

	u, err := user.New(name, email)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if u.Name != name {
		t.Errorf("expected name %q, got %q", name, u.Name)
	}
	if u.Email != email {
		t.Errorf("expected email %q, got %q", email, u.Email)
	}
	if u.ID.String() == "" {
		t.Error("expected valid ID, got empty string")
	}
}
