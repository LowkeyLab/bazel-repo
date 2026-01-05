package main

import (
	"context"
	"log/slog"
	"os"
	"strings"
	"time"

	"github.com/gin-contrib/cors"
	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	circlerepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	circlerest "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/rest"
	circleservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	contestcloser "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/closer"
	contestrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	contestrest "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/rest"
	contestservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	userrest "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/rest"
	userservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/healthcheck"
)

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

	var userRepo userrepo.Repository
	var contestRepo contestrepo.Repository
	var circleRepo circlerepo.Repository

	if devMode {
		slog.Info("Using in-memory repositories")
		circleRepo = circlerepo.NewMemory()
		contestRepo = contestrepo.NewMemory()
		userRepo = userrepo.NewMemory()
	} else {
		slog.Info("Using PostgreSQL repositories")
		connStr := os.Getenv("DATABASE_URL")
		if connStr == "" {
			connStr = "postgres://user:password@localhost:5432/predix?sslmode=disable"
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
	}

	// Create clock for services
	clk := clock.RealClock{}

	contestSvc := contestservice.NewService(contestRepo, clk)
	circleSvc := circleservice.NewService(circleRepo, userRepo, contestSvc)
	userSvc := userservice.NewService(userRepo)

	// Start the contest closer goroutine
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go contestcloser.StartCloser(ctx, contestRepo, clk, 10*time.Second)

	authManager := auth.NewManager(jwtSecret, 15*time.Minute)
	circleHandler := circlerest.NewHandler(circleSvc, contestSvc)
	contestHandler := contestrest.NewHandler(contestSvc, circleSvc)
	userHandler := userrest.NewHandler(userSvc, authManager)

	r := gin.Default()
	r.SetTrustedProxies(nil)
	r.Use(cors.New(cors.Config{
		AllowOrigins:     []string{"http://localhost:4200"},
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
	contestHandler.RegisterRoutes(protected)
	healthcheck.RegisterRoutes(r)

	slog.Info("Predix service starting", "address", ":8080")
	if err := r.Run(":8080"); err != nil {
		slog.Error("server exited", "error", err)
		os.Exit(1)
	}
}
