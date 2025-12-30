package contest

import (
	"context"
	"fmt"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// PostgresRepository is a PostgreSQL implementation of the Repository interface.
type PostgresRepository struct {
	pool    *pgxpool.Pool
	queries *db.Queries
}

// NewPostgresRepository creates a new PostgresRepository.
func NewPostgresRepository(pool *pgxpool.Pool) *PostgresRepository {
	return &PostgresRepository{
		pool:    pool,
		queries: db.New(pool),
	}
}

// Save persists a Contest with its options and predictions to the database.
func (r *PostgresRepository) Save(ctx context.Context, contest *Contest) error {
	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return fmt.Errorf("failed to begin transaction: %w", err)
	}
	defer tx.Rollback(ctx)

	qtx := r.queries.WithTx(tx)

	// Save contest
	_, err = qtx.CreateContest(ctx, db.CreateContestParams{
		ID:        uuid.UUID(contest.ID),
		CircleID:  uuid.UUID(contest.CircleID),
		CreatorID: uuid.UUID(contest.CreatorID),
		Question:  contest.Question,
		Status:    string(contest.Status),
		CreatedAt: pgtype.Timestamp{Time: contest.CreatedAt, Valid: true},
		ExpiresAt: pgtype.Timestamp{Time: contest.ExpiresAt, Valid: true},
	})
	if err != nil {
		return fmt.Errorf("failed to save contest: %w", err)
	}

	// Save options
	for _, option := range contest.Options {
		err = qtx.CreateOption(ctx, db.CreateOptionParams{
			ContestID: uuid.UUID(contest.ID),
			OptionID:  int32(option.ID),
			Text:      option.Text,
		})
		if err != nil {
			return fmt.Errorf("failed to save option: %w", err)
		}
	}

	// Save predictions
	for _, prediction := range contest.Predictions {
		_, err = qtx.CreatePrediction(ctx, db.CreatePredictionParams{
			ContestID: uuid.UUID(contest.ID),
			UserID:    uuid.UUID(prediction.UserID),
			OptionID:  int32(prediction.OptionID),
			Clout:     int32(prediction.Clout),
			CreatedAt: pgtype.Timestamp{Time: prediction.Timestamp, Valid: true},
		})
		if err != nil {
			return fmt.Errorf("failed to save prediction: %w", err)
		}
	}

	// Update result if resolved
	if contest.Status == StatusResolved && contest.ResultOptionID != nil {
		err = qtx.UpdateContestStatus(ctx, db.UpdateContestStatusParams{
			ID:             uuid.UUID(contest.ID),
			Status:         string(contest.Status),
			ResultOptionID: pgtype.Int4{Int32: int32(*contest.ResultOptionID), Valid: true},
		})
		if err != nil {
			return fmt.Errorf("failed to update contest status: %w", err)
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return fmt.Errorf("failed to commit transaction: %w", err)
	}

	return nil
}

// FindByID retrieves a Contest with all its options and predictions by ID.
func (r *PostgresRepository) FindByID(ctx context.Context, id ID) (*Contest, error) {
	dbContest, err := r.queries.GetContest(ctx, uuid.UUID(id))
	if err != nil {
		return nil, fmt.Errorf("failed to find contest by id: %w", err)
	}

	// Load options
	dbOptions, err := r.queries.ListContestOptions(ctx, uuid.UUID(id))
	if err != nil {
		return nil, fmt.Errorf("failed to load contest options: %w", err)
	}

	options := make(map[int]*Option)
	for _, dbOption := range dbOptions {
		options[int(dbOption.OptionID)] = &Option{
			ID:   int(dbOption.OptionID),
			Text: dbOption.Text,
		}
	}

	// Load predictions
	dbPredictions, err := r.queries.ListContestPredictions(ctx, uuid.UUID(id))
	if err != nil {
		return nil, fmt.Errorf("failed to load contest predictions: %w", err)
	}

	predictions := make([]*Prediction, len(dbPredictions))
	for i, dbPrediction := range dbPredictions {
		predictions[i] = &Prediction{
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

	return &Contest{
		ID:             ID(dbContest.ID),
		CircleID:       circle.ID(dbContest.CircleID),
		CreatorID:      user.ID(dbContest.CreatorID),
		Question:       dbContest.Question,
		Options:        options,
		Predictions:    predictions,
		Status:         Status(dbContest.Status),
		ResultOptionID: resultOptionID,
		CreatedAt:      dbContest.CreatedAt.Time,
		ExpiresAt:      dbContest.ExpiresAt.Time,
	}, nil
}

// FindByCircleID retrieves all Contests for a given Circle.
func (r *PostgresRepository) FindByCircleID(ctx context.Context, circleID circle.ID) ([]*Contest, error) {
	dbContests, err := r.queries.ListContestsByCircle(ctx, uuid.UUID(circleID))
	if err != nil {
		return nil, fmt.Errorf("failed to find contests by circle id: %w", err)
	}

	contests := make([]*Contest, len(dbContests))
	for i, dbContest := range dbContests {
		// Load options
		dbOptions, err := r.queries.ListContestOptions(ctx, dbContest.ID)
		if err != nil {
			return nil, fmt.Errorf("failed to load contest options: %w", err)
		}

		options := make(map[int]*Option)
		for _, dbOption := range dbOptions {
			options[int(dbOption.OptionID)] = &Option{
				ID:   int(dbOption.OptionID),
				Text: dbOption.Text,
			}
		}

		// Load predictions
		dbPredictions, err := r.queries.ListContestPredictions(ctx, dbContest.ID)
		if err != nil {
			return nil, fmt.Errorf("failed to load contest predictions: %w", err)
		}

		predictions := make([]*Prediction, len(dbPredictions))
		for j, dbPrediction := range dbPredictions {
			predictions[j] = &Prediction{
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

		contests[i] = &Contest{
			ID:             ID(dbContest.ID),
			CircleID:       circle.ID(dbContest.CircleID),
			CreatorID:      user.ID(dbContest.CreatorID),
			Question:       dbContest.Question,
			Options:        options,
			Predictions:    predictions,
			Status:         Status(dbContest.Status),
			ResultOptionID: resultOptionID,
			CreatedAt:      dbContest.CreatedAt.Time,
			ExpiresAt:      dbContest.ExpiresAt.Time,
		}
	}

	return contests, nil
}
