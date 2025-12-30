package main

import (
	"fmt"
	"log"

	"github.com/gin-gonic/gin"
	"github.com/lowkeylab/bazel-repo/predix/internal/core/application"
	"github.com/lowkeylab/bazel-repo/predix/internal/infrastructure/inmemory"
	"github.com/lowkeylab/bazel-repo/predix/internal/transport/http"
)

func main() {
	// 1. Initialize Infrastructure
	userRepo := inmemory.NewUserRepository()
	circleRepo := inmemory.NewCircleRepository()
	contestRepo := inmemory.NewContestRepository()

	// 2. Initialize Application Service
	svc := application.NewService(userRepo, circleRepo, contestRepo)

	// 3. Initialize HTTP Transport
	handler := http.NewHandler(svc)
	r := gin.Default()
	handler.RegisterRoutes(r)

	fmt.Println("Predix service starting on :8080...")
	if err := r.Run(":8080"); err != nil {
		log.Fatalf("failed to run server: %v", err)
	}
}
