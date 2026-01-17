package main

import (
	"context"
	"database/sql"
	"log/slog"
	"os"
	"strings"
	"time"

	"github.com/gin-contrib/cors"
	sloggin "github.com/gin-contrib/slog"
	"github.com/gin-gonic/gin"
	_ "github.com/jackc/pgx/v5/stdlib"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	circlerepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	circlerest "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/rest"
	circleservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/sse"
	contestcloser "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/closer"
	contestrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	userrest "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/rest"
	userservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/healthcheck"
	"github.com/lowkeylab/bazel-repo/predix/internal/migrations"
)

type PostgresTxManager struct {
	db *sql.DB
}

type UserRepository interface {
	userservice.UserRepository
	circleservice.UserRepository
}

type ContestRepository interface {
	circleservice.ContestRepository
}

func (m *PostgresTxManager) RunInTx(ctx context.Context, fn func(ctx context.Context) error) error {
	return db.RunInTx(ctx, m.db, fn)
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

		if err := migrations.Run(connStr); err != nil {
			slog.Error("failed to run migrations", "error", err)
			os.Exit(1)
		}

		sqlDB, err := sql.Open("pgx", connStr)
		if err != nil {
			slog.Error("unable to connect to database", "error", err)
			os.Exit(1)
		}
		defer sqlDB.Close()

		circleRepo = circlerepo.NewPostgres(sqlDB)
		contestRepo = contestrepo.NewPostgres(sqlDB)
		userRepo = userrepo.NewPostgres(sqlDB)
		txManager = &PostgresTxManager{db: sqlDB}
	}

	// Create clock for services
	clk := clock.RealClock{}

	sseManager := sse.NewManager()

	circleSvc := circleservice.NewService(
		circleRepo,
		userRepo,
		contestRepo,
		clk,
		txManager,
		sseManager,
	)
	userSvc := userservice.NewService(userRepo)

	// Start the contest closer goroutine
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go contestcloser.StartCloser(ctx, circleSvc, 10*time.Minute)

	authManager := auth.NewManager(jwtSecret, 15*time.Minute)
	circleHandler := circlerest.NewHandler(circleSvc, sseManager)
	userHandler := userrest.NewHandler(userSvc, authManager)

	r := gin.New()
	r.Use(sloggin.SetLogger(sloggin.WithWriter(os.Stdout)))
	r.Use(gin.Recovery())
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
