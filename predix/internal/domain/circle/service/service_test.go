package service_test

import (
	"context"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// createTestUser creates a test user in the database
func createTestUser(t *testing.T, pool *pgxpool.Pool, username string) user.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)
	passwordHash := "hash"

	result, err := q.CreateUser(ctx, db.CreateUserParams{
		Username:     username,
		PasswordHash: passwordHash,
		Role:         db.UserRoleMember,
	})
	require.NoError(t, err)

	return user.ID(result.ID)
}

func TestCreateCircle(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	svc := service.NewService(repo)
	q := db.New(pool)

	// Create a user to be the circle creator
	creatorID := createTestUser(t, pool, "alice")

	// Test: Create a circle
	c, err := svc.CreateCircle(ctx, "Book Club", creatorID)
	require.NoError(t, err)
	assert.NotNil(t, c)
	assert.Equal(t, "Book Club", c.Name)
	assert.Len(t, c.Members, 1)

	// Verify in database
	dbCircle, err := q.GetCircle(ctx, int32(c.ID))
	require.NoError(t, err)
	assert.Equal(t, "Book Club", dbCircle.Name)

	// Verify creator is a member with initial clout
	dbMember, err := q.GetCircleMember(ctx, db.GetCircleMemberParams{
		CircleID: int32(c.ID),
		UserID:   int32(creatorID),
	})
	require.NoError(t, err)
	assert.Equal(t, int32(1000), dbMember.Clout)
}

func TestCreateCircle_WithEmptyName(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	svc := service.NewService(repo)

	creatorID := createTestUser(t, pool, "bob")

	// Test: Create circle with empty name should fail
	c, err := svc.CreateCircle(ctx, "", creatorID)
	assert.Error(t, err)
	assert.Nil(t, c)
}
