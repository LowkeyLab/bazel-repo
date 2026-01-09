package testutil

import (
	"context"
	"fmt"
	"path/filepath"
	"testing"
	"time"

	"github.com/bazelbuild/rules_go/go/runfiles"
	"github.com/golang-migrate/migrate/v4"
	_ "github.com/golang-migrate/migrate/v4/database/postgres"
	_ "github.com/golang-migrate/migrate/v4/source/file"
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

	// Apply Schema via Migrations
	if err := runMigrations(connStr); err != nil {
		t.Fatalf("failed to run migrations: %v", err)
	}

	return pool
}

// runMigrations applies migrations to the test database
func runMigrations(connStr string) error {
	// Try using Bazel runfiles to locate migrations directory
	// The path should be relative to the repository root.
	// For example: predix/internal/sql/migrations

	// We need to resolve one file in the directory to find the absolute path to the directory
	// because runfiles.Rlocation expects a file path usually.
	// However, let's try to find the directory by resolving a known file inside it.
	// Since we are using filegroup glob(["migrations/*.sql"]), we can look for "000001_init_schema.up.sql"

	migrationFile := "predix/internal/sql/migrations/000001_init_schema.up.sql"
	// Attempt to resolve with module name prefix if necessary.
	// Usually it is "_main/predix/..." or just "predix/..." depending on how rules_go sets it up.
	// Based on the previous code, it used "_main/predix/internal/sql/schema.sql"

	rloc, err := runfiles.Rlocation("_main/" + migrationFile)
	if err != nil {
		// Fallback without _main
		rloc, err = runfiles.Rlocation(migrationFile)
		if err != nil {
			return fmt.Errorf("could not locate migration file %s: %v", migrationFile, err)
		}
	}

	migrationsDir := filepath.Dir(rloc)

	m, err := migrate.New(
		"file://"+migrationsDir,
		connStr,
	)
	if err != nil {
		return err
	}

	if err := m.Up(); err != nil && err != migrate.ErrNoChange {
		return err
	}

	return nil
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
	_, err := pool.Exec(ctx, `TRUNCATE TABLE predictions, options, contests, circle_members, circles RESTART IDENTITY CASCADE`)
	if err != nil {
		t.Fatalf("failed to reset tables: %v", err)
	}
}
