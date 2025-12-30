package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/core/application"
	"github.com/lowkeylab/bazel-repo/predix/infrastructure/inmemory"
)

func main() {
	ctx := context.Background()

	// 1. Initialize Infrastructure
	userRepo := inmemory.NewUserRepository()
	circleRepo := inmemory.NewCircleRepository()
	predictionRepo := inmemory.NewPredictionRepository()

	// 2. Initialize Application Service
	svc := application.NewService(userRepo, circleRepo, predictionRepo)

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

	// Alice creates a prediction
	p, err := svc.CreatePrediction(ctx,
		c.ID.String(),
		alice.ID.String(),
		"Who will win the game?",
		[]string{"Team A", "Team B"},
		time.Now().Add(24*time.Hour),
	)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Created Prediction: %s\n", p.Question)

	// Find option IDs
	var optionA string
	for id, opt := range p.Options {
		if opt.Text == "Team A" {
			optionA = id
		}
	}

	// Bob bets on Team A
	if err := svc.PlaceBet(ctx, p.ID.String(), bob.ID.String(), optionA, 50); err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Bob placed a bet on Team A\n")

	// Alice resolves (Team A wins)
	if err := svc.ResolvePrediction(ctx, p.ID.String(), optionA); err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Prediction resolved! Winner: Team A\n")
}
