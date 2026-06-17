package testutil

import (
	"context"
	"database/sql"
	"testing"
	"time"

	_ "github.com/jackc/pgx/v5/stdlib"
	"github.com/lowkeylab/bazel-repo/predix/internal/migrations"
	"github.com/testcontainers/testcontainers-go"
	"github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

// SetupTestDB starts a PostgreSQL container, applies the schema, and returns a connection pool.
func SetupTestDB(t *testing.T) *sql.DB {
	t.Helper()

	if testing.Short() {
		t.Skip("skipping integration test")
	}

	ctx := context.Background()

	// Start Postgres Container
	pgContainer, err := postgres.Run(
		ctx,
		"postgres:18",
		postgres.WithDatabase("predix"),
		postgres.WithUsername("user"),
		postgres.WithPassword("password"),
		testcontainers.WithWaitStrategy(
			wait.ForLog("database system is ready to accept connections").
				WithOccurrence(2).
				WithStartupTimeout(5*time.Second),
		),
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

	db, err := sql.Open("pgx", connStr)
	if err != nil {
		t.Fatalf("failed to open database connection: %v", err)
	}

	t.Cleanup(func() {
		db.Close()
	})

	// Apply Schema via Migrations
	if err := migrations.Run(connStr); err != nil {
		t.Fatalf("failed to run migrations: %v", err)
	}

	return db
}

// WithTestDB starts the Postgres container once for a group of subtests.
// Call ResetTables at the start of each subtest to isolate data.
func WithTestDB(t *testing.T, fn func(t *testing.T, db *sql.DB)) {
	t.Helper()

	db := SetupTestDB(t)
	fn(t, db)
}

// ResetTables truncates all application tables while keeping the container alive.
func ResetTables(t *testing.T, db *sql.DB) {
	t.Helper()

	ctx := context.Background()
	_, err := db.ExecContext(ctx, `TRUNCATE TABLE predictions, options, contests, circle_members, circles, users RESTART IDENTITY CASCADE`)
	if err != nil {
		t.Fatalf("failed to reset tables: %v", err)
	}
}
