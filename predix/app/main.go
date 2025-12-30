package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/core/application"
	"github.com/lowkeylab/bazel-repo/predix/internal/infrastructure/inmemory"
)

func main() {
	ctx := context.Background()

	// 1. Initialize Infrastructure
	userRepo := inmemory.NewUserRepository()
	circleRepo := inmemory.NewCircleRepository()
	contestRepo := inmemory.NewContestRepository()

	// 2. Initialize Application Service
	svc := application.NewService(userRepo, circleRepo, contestRepo)

	// 3. Scenario

	// Create Users
	alice, err := svc.CreateUser(ctx, "Alice", "alice@example.com")
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Created User: %s (%s)\n", alice.Name, alice.ID)

	bob, err := svc.CreateUser(ctx, "Bob", "bob@example.com")
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Created User: %s (%s)\n", bob.Name, bob.ID)

	// Alice creates a Circle
	c, err := svc.CreateCircle(ctx, "The Sunday Football Crew", alice.ID.String())
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Created Circle: %s (Code: %s)\n", c.Name, c.InviteCode)

	// Bob joins
	if err := svc.JoinCircle(ctx, bob.ID.String(), c.InviteCode); err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Bob joined the circle\n")

	// Alice creates a contest
	contest, err := svc.CreateContest(ctx,
		c.ID.String(),
		alice.ID.String(),
		"Who will win the game?",
		[]string{"Team A", "Team B"},
		time.Now().Add(24*time.Hour),
	)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Created Contest: %s\n", contest.Question)

	// Find option IDs
	var optionA int
	for id, opt := range contest.Options {
		if opt.Text == "Team A" {
			optionA = id
		}
	}

	// Bob predicts Team A
	if err := svc.Predict(ctx, contest.ID.String(), bob.ID.String(), optionA, 50); err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Bob predicted Team A\n")

	// Alice resolves (Team A wins)
	if err := svc.ResolveContest(ctx, contest.ID.String(), optionA); err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Contest resolved! Winner: Team A\n")
}
