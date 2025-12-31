package main

import (
	"context"
	"fmt"
	"log"
	"os"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5/pgxpool"
	circlerest "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/rest"
	circleservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	contestrest "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/rest"
	contestservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/healthcheck"
)

func main() {
	// 1. Initialize Infrastructure
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

	// 2. Initialize Application Services
	circleSvc := circleservice.NewService(pool)
	contestSvc := contestservice.NewService(pool)

	// 3. Initialize HTTP Handlers
	circleHandler := circlerest.NewHandler(circleSvc)
	contestHandler := contestrest.NewHandler(contestSvc)

	// 4. Setup HTTP Router
	r := gin.Default()
	circleHandler.RegisterRoutes(r)
	contestHandler.RegisterRoutes(r)
	healthcheck.RegisterRoutes(r)

	fmt.Println("Predix service starting on :8080...")
	if err := r.Run(":8080"); err != nil {
		log.Fatalf("failed to run server: %v", err)
	}
}
