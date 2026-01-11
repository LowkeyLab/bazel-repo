package main

import (
	"context"
	"embed"
	"fmt"
	"log/slog"
	"os"
	"strings"
	"time"

	"github.com/gin-contrib/cors"
	"github.com/gin-gonic/gin"
	"github.com/golang-migrate/migrate/v4"
	_ "github.com/golang-migrate/migrate/v4/database/postgres"
	"github.com/golang-migrate/migrate/v4/source/iofs"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	circlerepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	circlerest "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/rest"
	circleservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	contestcloser "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/closer"
	contestrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	userrest "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/rest"
	userservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/healthcheck"
)

//go:embed migrations/*.sql
var migrationFS embed.FS

func runMigrations(connStr string) error {
	sourceDriver, err := iofs.New(migrationFS, "migrations")
	if err != nil {
		return fmt.Errorf("failed to create iofs driver: %w", err)
	}

	m, err := migrate.NewWithSourceInstance("iofs", sourceDriver, connStr)
	if err != nil {
		return fmt.Errorf("failed to create migrate instance: %w", err)
	}

	if err := m.Up(); err != nil && err != migrate.ErrNoChange {
		return fmt.Errorf("failed to run migrations: %w", err)
	}

	return nil
}

type PostgresTxManager struct {
	pool *pgxpool.Pool
}

type UserRepository interface {
	userservice.UserRepository
	circleservice.UserRepository
}

type ContestRepository interface {
	circleservice.ContestRepository
}

func (m *PostgresTxManager) RunInTx(ctx context.Context, fn func(ctx context.Context) error) error {
	return db.RunInTx(ctx, m.pool, fn)
}

type NoOpTxManager struct{}

func (m *NoOpTxManager) RunInTx(ctx context.Context, fn func(ctx context.Context) error) error {
	return fn(ctx)
}

func main() {
	logHandler := slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo})
	logger := slog.New(logHandler)
	slog.SetDefault(logger)

	devMode := strings.ToLower(os.Getenv("DEV_MODE")) == "true"
	jwtSecret := os.Getenv("JWT_SECRET")
	if devMode {
		slog.Info("Running in development mode with in-memory storage")
		jwtSecret = "dev-secret"
	}

	var userRepo UserRepository
	var contestRepo ContestRepository
	var circleRepo circleservice.CircleRepository
	var txManager circleservice.TransactionManager

	if devMode {
		slog.Info("Using in-memory repositories")
		circleRepo = circlerepo.NewMemory()
		contestRepo = contestrepo.NewMemory()
		userRepo = userrepo.NewMemory()
		txManager = &NoOpTxManager{}
	} else {
		slog.Info("Using PostgreSQL repositories")
		connStr, ok := os.LookupEnv("DATABASE_URL")
		if !ok {
			slog.Error("DATABASE_URL environment variable is required in production mode")
			os.Exit(1)
		}

		if err := runMigrations(connStr); err != nil {
			slog.Error("failed to run migrations", "error", err)
			os.Exit(1)
		}

		pool, err := pgxpool.New(context.Background(), connStr)
		if err != nil {
			slog.Error("unable to connect to database", "error", err)
			os.Exit(1)
		}
		defer pool.Close()

		circleRepo = circlerepo.NewPostgres(pool)
		contestRepo = contestrepo.NewPostgres(pool)
		userRepo = userrepo.NewPostgres(pool)
		txManager = &PostgresTxManager{pool: pool}
	}

	// Create clock for services
	clk := clock.RealClock{}

	circleSvc := circleservice.NewService(
		circleRepo,
		userRepo,
		contestRepo,
		clk,
		txManager,
	)
	userSvc := userservice.NewService(userRepo)

	// Start the contest closer goroutine
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go contestcloser.StartCloser(ctx, circleSvc, 10*time.Minute)

	authManager := auth.NewManager(jwtSecret, 15*time.Minute)
	circleHandler := circlerest.NewHandler(circleSvc)
	userHandler := userrest.NewHandler(userSvc, authManager)

	r := gin.Default()
	r.SetTrustedProxies(nil)

	var allowOrigins []string
	if origins := os.Getenv("ALLOWED_ORIGINS"); origins != "" {
		allowOrigins = strings.Split(origins, ",")
	}
	if devMode {
		allowOrigins = append(allowOrigins, "http://localhost:4200")
	}

	r.Use(cors.New(cors.Config{
		AllowOrigins:     allowOrigins,
		AllowMethods:     []string{"GET", "POST", "PUT", "DELETE"},
		AllowHeaders:     []string{"Origin", "Content-Type", "Authorization"},
		ExposeHeaders:    []string{"Content-Length"},
		AllowCredentials: true,
		MaxAge:           12 * time.Hour,
	}))

	userHandler.RegisterRoutes(r)

	protected := r.Group("/protected")
	protected.Use(authManager.Middleware())
	circleHandler.RegisterRoutes(protected)
	healthcheck.RegisterRoutes(r)

	slog.Info("Predix service starting", "address", ":8080")
	if err := r.Run(":8080"); err != nil {
		slog.Error("server exited", "error", err)
		os.Exit(1)
	}
}
