package service_test

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/testcontainers/testcontainers-go"
	"github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

func TestService_CreateContest(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test")
	}

	ctx := context.Background()

	// 1. Start Postgres Container
	pgContainer, err := postgres.Run(ctx,
		"postgres:16-alpine",
		postgres.WithDatabase("predix"),
		postgres.WithUsername("user"),
		postgres.WithPassword("password"),
		testcontainers.WithWaitStrategy(
			wait.ForLog("database system is ready to accept connections").
				WithOccurrence(2).
				WithStartupTimeout(5*time.Second)),
	)
	require.NoError(t, err)
	defer func() {
		if err := pgContainer.Terminate(ctx); err != nil {
			t.Fatalf("failed to terminate container: %s", err)
		}
	}()

	connStr, err := pgContainer.ConnectionString(ctx, "sslmode=disable")
	require.NoError(t, err)

	pool, err := pgxpool.New(ctx, connStr)
	require.NoError(t, err)
	defer pool.Close()

	// 2. Apply Schema
	schemaPath := "../../sql/schema.sql"
	schemaContent, err := os.ReadFile(schemaPath)
	if err != nil {
		// Fallback for running from root or different context
		schemaPath = "predix/internal/sql/schema.sql"
		schemaContent, err = os.ReadFile(schemaPath)
		if err != nil {
			schemaPath = "../../../sql/schema.sql"
			schemaContent, err = os.ReadFile(schemaPath)
		}
	}
	require.NoError(t, err, "could not read schema file from %s or %s or %s", "../../sql/schema.sql", "predix/internal/sql/schema.sql", "../../../sql/schema.sql")

	_, err = pool.Exec(ctx, string(schemaContent))
	require.NoError(t, err)

	// 3. Setup Dependencies (User, Circle)
	// We need to create a user and a circle first because of foreign keys.
	q := db.New(pool)

	userID := uuid.New()
	_, err = q.CreateUser(ctx, db.CreateUserParams{
		ID:    userID,
		Name:  "Test User",
		Email: "test@example.com",
	})
	require.NoError(t, err)

	circleID := uuid.New()
	_, err = q.CreateCircle(ctx, db.CreateCircleParams{
		ID:         circleID,
		Name:       "Test Circle",
		InviteCode: "TEST12",
		CreatedAt:  pgtype.Timestamp{Time: time.Now(), Valid: true},
	})
	require.NoError(t, err)

	// 4. Test Service
	svc := service.NewService(pool)

	opts := []string{"Yes", "No", "Maybe"}
	expiresAt := time.Now().Add(24 * time.Hour)

	c, err := svc.CreateContest(ctx, circle.ID(circleID), user.ID(userID), "Is this working?", opts, expiresAt)
	require.NoError(t, err)
	assert.NotNil(t, c)
	assert.Equal(t, "Is this working?", c.Question)
	assert.Len(t, c.Options, 3)

	// Verify in DB
	dbContest, err := q.GetContest(ctx, uuid.UUID(c.ID))
	require.NoError(t, err)
	assert.Equal(t, "Is this working?", dbContest.Question)

	dbOptions, err := q.ListContestOptions(ctx, uuid.UUID(c.ID))
	require.NoError(t, err)
	assert.Len(t, dbOptions, 3)
}
