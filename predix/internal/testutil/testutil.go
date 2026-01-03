package testutil

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/bazelbuild/rules_go/go/runfiles"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/testcontainers/testcontainers-go"
	"github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

// SetupTestDB starts a PostgreSQL container, applies the schema, and returns a connection pool.
// If schemaPath is empty, it will use runfiles to locate the schema.
func SetupTestDB(t *testing.T) *pgxpool.Pool {
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
	if err != nil {
		t.Fatalf("failed to start postgres container: %v", err)
	}

	t.Cleanup(func() {
		if err := pgContainer.Terminate(ctx); err != nil {
			t.Logf("failed to terminate container: %v", err)
		}
	})

	connStr, err := pgContainer.ConnectionString(ctx, "sslmode=disable")
	if err != nil {
		t.Fatalf("failed to get connection string: %v", err)
	}

	pool, err := pgxpool.New(ctx, connStr)
	if err != nil {
		t.Fatalf("failed to create connection pool: %v", err)
	}

	t.Cleanup(func() {
		pool.Close()
	})

	// Apply Schema
	schemaContent, err := readSchemaFile()
	if err != nil {
		t.Fatalf("could not read schema file: %v", err)
	}

	_, err = pool.Exec(ctx, string(schemaContent))
	if err != nil {
		t.Fatalf("failed to apply schema: %v", err)
	}

	return pool
}

// WithTestDB starts the Postgres container once for a group of subtests.
// Call ResetTables at the start of each subtest to isolate data.
func WithTestDB(t *testing.T, fn func(t *testing.T, pool *pgxpool.Pool)) {
	t.Helper()

	pool := SetupTestDB(t)
	fn(t, pool)
}

// ResetTables truncates all application tables while keeping the container alive.
func ResetTables(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()

	ctx := context.Background()
	_, err := pool.Exec(ctx, `TRUNCATE TABLE predictions, options, contest_circles, contests, circle_members, circles, users RESTART IDENTITY CASCADE`)
	if err != nil {
		t.Fatalf("failed to reset tables: %v", err)
	}
}

// readSchemaFile tries to read the schema file from the provided path or using runfiles
func readSchemaFile() ([]byte, error) {
	// Try using Bazel runfiles
	rloc, err := runfiles.Rlocation("_main/predix/internal/sql/schema.sql")
	if err != nil {
		return nil, err
	}

	content, err := os.ReadFile(rloc)
	if err != nil {
		return nil, err
	}

	return content, nil
}
