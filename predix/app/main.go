package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"strings"
	"time"

	"github.com/gin-contrib/cors"
	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	circlerepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	circlerest "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/rest"
	circleservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	contestrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	contestrest "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/rest"
	contestservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	userrest "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/rest"
	userservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/healthcheck"
)

func main() {
	// 1. Initialize Infrastructure
	// Determine whether to use in-memory or PostgreSQL repositories
	useInMemory := strings.ToLower(os.Getenv("USE_IN_MEMORY")) == "true"

	// 2. Initialize Repositories
	var circleSvc *circleservice.Service
	var contestSvc *contestservice.Service
	var userSvc *userservice.Service

	if useInMemory {
		fmt.Println("Using in-memory repositories...")
		circleRepo := circlerepo.NewMemory()
		contestRepo := contestrepo.NewMemory()
		userRepo := userrepo.NewMemory()
		circleSvc = circleservice.NewService(circleRepo, userRepo)
		contestSvc = contestservice.NewService(contestRepo)
		userSvc = userservice.NewService(userRepo)
	} else {
		fmt.Println("Using PostgreSQL repositories...")
		// DB
		connStr := os.Getenv("DATABASE_URL")
		if connStr == "" {
			connStr = "postgres://user:password@localhost:5432/predix?sslmode=disable"
		}

		pool, err := pgxpool.New(context.Background(), connStr)
		if err != nil {
			log.Fatalf("Unable to connect to database: %v", err)
		}
		defer pool.Close()

		circleRepo := circlerepo.NewPostgres(pool)
		contestRepo := contestrepo.NewPostgres(pool)
		userRepo := userrepo.NewPostgres(pool)
		circleSvc = circleservice.NewService(circleRepo, userRepo)
		contestSvc = contestservice.NewService(contestRepo)
		userSvc = userservice.NewService(userRepo)
	}

	// 3. Initialize HTTP Handlers
	authManager := auth.NewManager(os.Getenv("JWT_SECRET"), 24*time.Hour)
	circleHandler := circlerest.NewHandler(circleSvc)
	contestHandler := contestrest.NewHandler(contestSvc)
	userHandler := userrest.NewHandler(userSvc, authManager)

	// 4. Setup HTTP Router
	r := gin.Default()
	r.SetTrustedProxies(nil)
	r.Use(cors.Default())

	userHandler.RegisterRoutes(r)

	protected := r.Group("/protected")
	protected.Use(authManager.Middleware())
	circleHandler.RegisterRoutes(protected)
	contestHandler.RegisterRoutes(protected)
	healthcheck.RegisterRoutes(r)

	fmt.Println("Predix service starting on :8080...")
	if err := r.Run(":8080"); err != nil {
		log.Fatalf("failed to run server: %v", err)
	}
}
