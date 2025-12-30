package main

import (
	"fmt"

	"github.com/lowkeylab/bazel-repo/predix/internal/core/application"
	"github.com/lowkeylab/bazel-repo/predix/internal/infrastructure/inmemory"
)

func main() {
	// 1. Initialize Infrastructure
	userRepo := inmemory.NewUserRepository()
	circleRepo := inmemory.NewCircleRepository()
	contestRepo := inmemory.NewContestRepository()

	// 2. Initialize Application Service
	_ = application.NewService(userRepo, circleRepo, contestRepo)

	fmt.Println("Predix service initialized.")
}
