package repository

import (
	"context"
	"fmt"
	"math/big"

	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Postgres is a PostgreSQL implementation of the Repository interface.
type Postgres struct {
	pool    *pgxpool.Pool
	queries *db.Queries
}

// NewPostgres creates a new Postgres repository.
func NewPostgres(pool *pgxpool.Pool) *Postgres {
	return &Postgres{
		pool:    pool,
		queries: db.New(pool),
	}
}

// Save persists a Contest with its options and predictions to the database.
// It handles both creation and updates.
func (r *Postgres) Save(ctx context.Context, c *contest.Contest) error {
	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return fmt.Errorf("failed to begin transaction: %w", err)
	}
	defer tx.Rollback(ctx)

	qtx := r.queries.WithTx(tx)

	// Check if contest exists (ID will be 0 for new contests)
	isNew := c.ID == 0

	if isNew {
		// Create new contest
		result, err := qtx.CreateContest(ctx, db.CreateContestParams{
			CircleID:        int32(c.CircleID),
			CreatorID:       int32(c.CreatorID),
			Question:        c.Question,
			Status:          string(c.Status),
			MinStake:        int32(c.MinStake),
			ConsumptionRate: pgtype.Numeric{Int: big.NewInt(1000), Exp: -2, Valid: true}, // 10.00
			CreatedAt:       pgtype.Timestamp{Time: c.CreatedAt, Valid: true},
			ClosesAt:        pgtype.Timestamp{Time: c.ClosesAt, Valid: true},
			Duration:        c.Duration,
		})
		if err != nil {
			return fmt.Errorf("failed to save contest: %w", err)
		}

		// Update contest with generated ID
		c.ID = contest.ID(result.ID)

		// Save options (only on creation)
		for _, option := range c.Options {
			err = qtx.CreateOption(ctx, db.CreateOptionParams{
				ContestID: int32(c.ID),
				OptionID:  int32(option.ID),
				Text:      option.Text,
			})
			if err != nil {
				return fmt.Errorf("failed to save option: %w", err)
			}
		}
	} else {
		// Update existing contest status
		resultOptionID := pgtype.Int4{Valid: false}
		if c.ResultOptionID != nil {
			resultOptionID = pgtype.Int4{Int32: int32(*c.ResultOptionID), Valid: true}
		}

		err = qtx.UpdateContestStatus(ctx, db.UpdateContestStatusParams{
			ID:             int32(c.ID),
			Status:         string(c.Status),
			ResultOptionID: resultOptionID,
		})
		if err != nil {
			return fmt.Errorf("failed to update contest: %w", err)
		}
	}

	// Save new predictions (compare with existing to avoid duplicates)
	existingPredictionCount := 0
	if !isNew {
		existingPredictions, _ := qtx.ListContestPredictions(ctx, int32(c.ID))
		existingPredictionCount = len(existingPredictions)
	}

	// Only save predictions that are new
	for i := existingPredictionCount; i < len(c.Predictions); i++ {
		prediction := c.Predictions[i]
		_, err = qtx.CreatePrediction(ctx, db.CreatePredictionParams{
			ContestID: int32(c.ID),
			UserID:    int32(prediction.UserID),
			OptionID:  int32(prediction.OptionID),
			Clout:     int32(prediction.Clout),
			CreatedAt: pgtype.Timestamp{Time: prediction.Timestamp, Valid: true},
		})
		if err != nil {
			return fmt.Errorf("failed to save prediction: %w", err)
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return fmt.Errorf("failed to commit transaction: %w", err)
	}

	return nil
}

// FindByID retrieves a Contest with all its options and predictions by ID.
func (r *Postgres) FindByID(ctx context.Context, id contest.ID) (*contest.Contest, error) {
	dbContest, err := r.queries.GetContest(ctx, int32(id))
	if err != nil {
		return nil, fmt.Errorf("failed to find contest by id: %w", err)
	}

	// Load options
	dbOptions, err := r.queries.ListContestOptions(ctx, int32(id))
	if err != nil {
		return nil, fmt.Errorf("failed to load contest options: %w", err)
	}

	options := make(map[int]*contest.Option)
	for _, dbOption := range dbOptions {
		options[int(dbOption.OptionID)] = &contest.Option{
			ID:   int(dbOption.OptionID),
			Text: dbOption.Text,
		}
	}

	// Load predictions
	dbPredictions, err := r.queries.ListContestPredictions(ctx, int32(id))
	if err != nil {
		return nil, fmt.Errorf("failed to load contest predictions: %w", err)
	}

	predictions := make([]*contest.Prediction, len(dbPredictions))
	for i, dbPrediction := range dbPredictions {
		predictions[i] = &contest.Prediction{
			UserID:    user.ID(dbPrediction.UserID),
			OptionID:  int(dbPrediction.OptionID),
			Clout:     int(dbPrediction.Clout),
			Timestamp: dbPrediction.CreatedAt.Time,
		}
	}

	// Parse result option ID
	var resultOptionID *int
	if dbContest.ResultOptionID.Valid {
		val := int(dbContest.ResultOptionID.Int32)
		resultOptionID = &val
	}

	minStake := int(dbContest.MinStake)
	if minStake == 0 {
		minStake = defaultMinStake
	}

	// Parse consumption rate from pgtype.Numeric
	consumptionRate := 0.10 // default
	if dbContest.ConsumptionRate.Valid {
		// Convert Numeric to string then parse - stored as percentage (10.00 = 10%)
		numStr := dbContest.ConsumptionRate.Int.String()
		if len(numStr) > 0 {
			// Simple conversion: if stored as 1000 with exp -2, that's 10.00
			// We need 0.10 for our float64 representation
			consumptionRate = float64(dbContest.ConsumptionRate.Int.Int64()) / 10000.0
		}
	}

	return &contest.Contest{
		ID:              contest.ID(dbContest.ID),
		CircleID:        circle.ID(dbContest.CircleID),
		CreatorID:       user.ID(dbContest.CreatorID),
		Question:        dbContest.Question,
		Options:         options,
		Predictions:     predictions,
		Status:          contest.Status(dbContest.Status),
		MinStake:        minStake,
		ConsumptionRate: consumptionRate,
		ResultOptionID:  resultOptionID,
		CreatedAt:       dbContest.CreatedAt.Time,
		ClosesAt:        dbContest.ClosesAt.Time,
		Duration:        dbContest.Duration,
	}, nil
}

// FindByCircleID retrieves all Contests for a given Circle.
func (r *Postgres) FindByCircleID(ctx context.Context, circleID circle.ID) ([]*contest.Contest, error) {
	dbContests, err := r.queries.ListContestsByCircle(ctx, int32(circleID))
	if err != nil {
		return nil, fmt.Errorf("failed to find contests by circle id: %w", err)
	}

	contests := make([]*contest.Contest, len(dbContests))
	for i, dbContest := range dbContests {
		// Load options
		dbOptions, err := r.queries.ListContestOptions(ctx, dbContest.ID)
		if err != nil {
			return nil, fmt.Errorf("failed to load contest options: %w", err)
		}

		options := make(map[int]*contest.Option)
		for _, dbOption := range dbOptions {
			options[int(dbOption.OptionID)] = &contest.Option{
				ID:   int(dbOption.OptionID),
				Text: dbOption.Text,
			}
		}

		// Load predictions
		dbPredictions, err := r.queries.ListContestPredictions(ctx, dbContest.ID)
		if err != nil {
			return nil, fmt.Errorf("failed to load contest predictions: %w", err)
		}

		predictions := make([]*contest.Prediction, len(dbPredictions))
		for j, dbPrediction := range dbPredictions {
			predictions[j] = &contest.Prediction{
				UserID:    user.ID(dbPrediction.UserID),
				OptionID:  int(dbPrediction.OptionID),
				Clout:     int(dbPrediction.Clout),
				Timestamp: dbPrediction.CreatedAt.Time,
			}
		}

		// Parse result option ID
		var resultOptionID *int
		if dbContest.ResultOptionID.Valid {
			val := int(dbContest.ResultOptionID.Int32)
			resultOptionID = &val
		}

		minStake := int(dbContest.MinStake)
		if minStake == 0 {
			minStake = defaultMinStake
		}

		contests[i] = &contest.Contest{
			ID:             contest.ID(dbContest.ID),
			CircleID:       circle.ID(dbContest.CircleID),
			CreatorID:      user.ID(dbContest.CreatorID),
			Question:       dbContest.Question,
			Options:        options,
			Predictions:    predictions,
			Status:         contest.Status(dbContest.Status),
			MinStake:       minStake,
			ResultOptionID: resultOptionID,
			CreatedAt:      dbContest.CreatedAt.Time,
			ClosesAt:       dbContest.ClosesAt.Time,
			Duration:       dbContest.Duration,
		}
	}

	return contests, nil
}

// FindExpiredContests retrieves all contests that are OPEN or LOCKED and have passed their closes_at time.
func (r *Postgres) FindExpiredContests(ctx context.Context) ([]*contest.Contest, error) {
	dbContests, err := r.queries.FindExpiredContests(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to find expired contests: %w", err)
	}

	contests := make([]*contest.Contest, len(dbContests))
	for i, dbContest := range dbContests {
		// Load options
		dbOptions, err := r.queries.ListContestOptions(ctx, dbContest.ID)
		if err != nil {
			return nil, fmt.Errorf("failed to load contest options: %w", err)
		}

		options := make(map[int]*contest.Option)
		for _, dbOption := range dbOptions {
			options[int(dbOption.OptionID)] = &contest.Option{
				ID:   int(dbOption.OptionID),
				Text: dbOption.Text,
			}
		}

		// Load predictions
		dbPredictions, err := r.queries.ListContestPredictions(ctx, dbContest.ID)
		if err != nil {
			return nil, fmt.Errorf("failed to load contest predictions: %w", err)
		}

		predictions := make([]*contest.Prediction, len(dbPredictions))
		for j, dbPrediction := range dbPredictions {
			predictions[j] = &contest.Prediction{
				UserID:    user.ID(dbPrediction.UserID),
				OptionID:  int(dbPrediction.OptionID),
				Clout:     int(dbPrediction.Clout),
				Timestamp: dbPrediction.CreatedAt.Time,
			}
		}

		// Parse result option ID
		var resultOptionID *int
		if dbContest.ResultOptionID.Valid {
			val := int(dbContest.ResultOptionID.Int32)
			resultOptionID = &val
		}

		minStake := int(dbContest.MinStake)
		if minStake == 0 {
			minStake = defaultMinStake
		}

		contests[i] = &contest.Contest{
			ID:             contest.ID(dbContest.ID),
			CircleID:       circle.ID(dbContest.CircleID),
			CreatorID:      user.ID(dbContest.CreatorID),
			Question:       dbContest.Question,
			Options:        options,
			Predictions:    predictions,
			Status:         contest.Status(dbContest.Status),
			MinStake:       minStake,
			ResultOptionID: resultOptionID,
			CreatedAt:      dbContest.CreatedAt.Time,
			ClosesAt:       dbContest.ClosesAt.Time,
			Duration:       dbContest.Duration,
		}
	}

	return contests, nil
}

// UpdateStatus updates the status of a contest.
func (r *Postgres) UpdateStatus(ctx context.Context, id contest.ID, status contest.Status) error {
	err := r.queries.UpdateContestStatusOnly(ctx, db.UpdateContestStatusOnlyParams{
		ID:     int32(id),
		Status: string(status),
	})
	if err != nil {
		return fmt.Errorf("failed to update contest status: %w", err)
	}
	return nil
}
