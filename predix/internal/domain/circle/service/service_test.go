package service_test

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/testcontainers/testcontainers-go"
	"github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

// setupTestDB creates a PostgreSQL container and returns a connection pool
func setupTestDB(t *testing.T) (*pgxpool.Pool, func()) {
	t.Helper()

	if testing.Short() {
		t.Skip("skipping integration test")
	}

	ctx := context.Background()

	// Start Postgres Container
	pgContainer, err := postgres.Run(ctx,
		"postgres:18",
		postgres.WithDatabase("predix"),
		postgres.WithUsername("user"),
		postgres.WithPassword("password"),
		testcontainers.WithWaitStrategy(
			wait.ForLog("database system is ready to accept connections").
				WithOccurrence(2).
				WithStartupTimeout(5*time.Second)),
	)
	require.NoError(t, err)

	connStr, err := pgContainer.ConnectionString(ctx, "sslmode=disable")
	require.NoError(t, err)

	pool, err := pgxpool.New(ctx, connStr)
	require.NoError(t, err)

	// Apply Schema
	schemaPath := "../../sql/schema.sql"
	schemaContent, err := os.ReadFile(schemaPath)
	if err != nil {
		schemaPath = "predix/internal/sql/schema.sql"
		schemaContent, err = os.ReadFile(schemaPath)
		if err != nil {
			schemaPath = "../../../sql/schema.sql"
			schemaContent, err = os.ReadFile(schemaPath)
		}
	}
	require.NoError(t, err, "could not read schema file")

	_, err = pool.Exec(ctx, string(schemaContent))
	require.NoError(t, err)

	// Return cleanup function
	cleanup := func() {
		pool.Close()
		if err := pgContainer.Terminate(ctx); err != nil {
			t.Logf("failed to terminate container: %s", err)
		}
	}

	return pool, cleanup
}

// createTestUser creates a test user in the database
func createTestUser(t *testing.T, pool *pgxpool.Pool, name, email string) user.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)

	userID := uuid.New()
	_, err := q.CreateUser(ctx, db.CreateUserParams{
		ID:    userID,
		Name:  name,
		Email: email,
	})
	require.NoError(t, err)

	return user.ID(userID)
}

func TestCreateCircle(t *testing.T) {
	pool, cleanup := setupTestDB(t)
	defer cleanup()

	ctx := context.Background()
	svc := service.NewService(pool)
	q := db.New(pool)

	// Create a user to be the circle creator
	creatorID := createTestUser(t, pool, "Alice", "alice@example.com")

	// Test: Create a circle
	c, err := svc.CreateCircle(ctx, "Book Club", creatorID)
	require.NoError(t, err)
	assert.NotNil(t, c)
	assert.Equal(t, "Book Club", c.Name)
	assert.NotEmpty(t, c.InviteCode)
	assert.Len(t, c.Members, 1)

	// Verify in database
	dbCircle, err := q.GetCircle(ctx, uuid.UUID(c.ID))
	require.NoError(t, err)
	assert.Equal(t, "Book Club", dbCircle.Name)

	// Verify creator is a member with initial clout
	dbMember, err := q.GetCircleMember(ctx, db.GetCircleMemberParams{
		CircleID: uuid.UUID(c.ID),
		UserID:   uuid.UUID(creatorID),
	})
	require.NoError(t, err)
	assert.Equal(t, int32(1000), dbMember.Clout)
}

func TestCreateCircle_WithEmptyName(t *testing.T) {
	pool, cleanup := setupTestDB(t)
	defer cleanup()

	ctx := context.Background()
	svc := service.NewService(pool)

	creatorID := createTestUser(t, pool, "Bob", "bob@example.com")

	// Test: Create circle with empty name should fail
	c, err := svc.CreateCircle(ctx, "", creatorID)
	assert.Error(t, err)
	assert.Nil(t, c)
}

func TestCreateCircle_GeneratesUniqueInviteCodes(t *testing.T) {
	pool, cleanup := setupTestDB(t)
	defer cleanup()

	ctx := context.Background()
	svc := service.NewService(pool)

	creatorID := createTestUser(t, pool, "Charlie", "charlie@example.com")

	// Test: Create multiple circles and verify unique invite codes
	circle1, err := svc.CreateCircle(ctx, "Circle 1", creatorID)
	require.NoError(t, err)

	circle2, err := svc.CreateCircle(ctx, "Circle 2", creatorID)
	require.NoError(t, err)

	assert.NotEqual(t, circle1.InviteCode, circle2.InviteCode)
}
