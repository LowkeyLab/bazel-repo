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
	schemaPath, err := runfiles.Rlocation("predix/internal/sql/schema.sql")
	if err != nil {
		t.Fatalf("Could not find schema file: %v\n", err)
	}
	schemaContent, err := os.ReadFile(schemaPath)
	if err != nil {
		t.Fatalf("Could not read schema file: %v\n", err)
	}

	_, err = pool.Exec(ctx, string(schemaContent))
	if err != nil {
		t.Fatalf("failed to apply schema: %v", err)
	}

	return pool
}
