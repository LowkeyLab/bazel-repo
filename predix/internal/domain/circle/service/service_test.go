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

func TestCircleService_CreateCircle(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test")
	}

	ctx := context.Background()

	// 1. Start Postgres Container
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

	q := db.New(pool)

	// Create User (Creator)
	userID := uuid.New()
	_, err = q.CreateUser(ctx, db.CreateUserParams{
		ID:    userID,
		Name:  "Circle Creator",
		Email: "creator@example.com",
	})
	require.NoError(t, err)

	// Test Service
	svc := service.NewService(pool)

	c, err := svc.CreateCircle(ctx, "My New Circle", user.ID(userID))
	require.NoError(t, err)
	assert.NotNil(t, c)
	assert.Equal(t, "My New Circle", c.Name)
	assert.NotEmpty(t, c.InviteCode)

	// Verify in DB
	dbCircle, err := q.GetCircle(ctx, uuid.UUID(c.ID))
	require.NoError(t, err)
	assert.Equal(t, "My New Circle", dbCircle.Name)

	// Verify Member
	dbMember, err := q.GetCircleMember(ctx, db.GetCircleMemberParams{
		CircleID: uuid.UUID(c.ID),
		UserID:   uuid.UUID(userID),
	})
	require.NoError(t, err)
	assert.Equal(t, int32(1000), dbMember.Clout)
}
