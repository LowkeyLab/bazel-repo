package repository

import (
	"context"
	"fmt"

	"github.com/google/uuid"
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

	// Check if contest exists
	_, err = qtx.GetContest(ctx, uuid.UUID(c.ID))
	isNew := err != nil

	if isNew {
		// Create new contest
		_, err = qtx.CreateContest(ctx, db.CreateContestParams{
			ID:        uuid.UUID(c.ID),
			CircleID:  uuid.UUID(c.CircleID),
			CreatorID: uuid.UUID(c.CreatorID),
			Question:  c.Question,
			Status:    string(c.Status),
			CreatedAt: pgtype.Timestamp{Time: c.CreatedAt, Valid: true},
			ExpiresAt: pgtype.Timestamp{Time: c.ExpiresAt, Valid: true},
		})
		if err != nil {
			return fmt.Errorf("failed to save contest: %w", err)
		}

		// Save options (only on creation)
		for _, option := range c.Options {
			err = qtx.CreateOption(ctx, db.CreateOptionParams{
				ContestID: uuid.UUID(c.ID),
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
			ID:             uuid.UUID(c.ID),
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
		existingPredictions, _ := qtx.ListContestPredictions(ctx, uuid.UUID(c.ID))
		existingPredictionCount = len(existingPredictions)
	}

	// Only save predictions that are new
	for i := existingPredictionCount; i < len(c.Predictions); i++ {
		prediction := c.Predictions[i]
		_, err = qtx.CreatePrediction(ctx, db.CreatePredictionParams{
			ContestID: uuid.UUID(c.ID),
			UserID:    uuid.UUID(prediction.UserID),
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
	dbContest, err := r.queries.GetContest(ctx, uuid.UUID(id))
	if err != nil {
		return nil, fmt.Errorf("failed to find contest by id: %w", err)
	}

	// Load options
	dbOptions, err := r.queries.ListContestOptions(ctx, uuid.UUID(id))
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
	dbPredictions, err := r.queries.ListContestPredictions(ctx, uuid.UUID(id))
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

	return &contest.Contest{
		ID:             contest.ID(dbContest.ID),
		CircleID:       circle.ID(dbContest.CircleID),
		CreatorID:      user.ID(dbContest.CreatorID),
		Question:       dbContest.Question,
		Options:        options,
		Predictions:    predictions,
		Status:         contest.Status(dbContest.Status),
		ResultOptionID: resultOptionID,
		CreatedAt:      dbContest.CreatedAt.Time,
		ExpiresAt:      dbContest.ExpiresAt.Time,
	}, nil
}

// FindByCircleID retrieves all Contests for a given Circle.
func (r *Postgres) FindByCircleID(ctx context.Context, circleID circle.ID) ([]*contest.Contest, error) {
	dbContests, err := r.queries.ListContestsByCircle(ctx, uuid.UUID(circleID))
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

		contests[i] = &contest.Contest{
			ID:             contest.ID(dbContest.ID),
			CircleID:       circle.ID(dbContest.CircleID),
			CreatorID:      user.ID(dbContest.CreatorID),
			Question:       dbContest.Question,
			Options:        options,
			Predictions:    predictions,
			Status:         contest.Status(dbContest.Status),
			ResultOptionID: resultOptionID,
			CreatedAt:      dbContest.CreatedAt.Time,
			ExpiresAt:      dbContest.ExpiresAt.Time,
		}
	}

	return contests, nil
}
