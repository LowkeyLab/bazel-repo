package repository

import (
	"context"
	"database/sql"
	"fmt"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Postgres is a PostgreSQL implementation of contest repository interfaces.
type Postgres struct {
	db      *sql.DB
	queries *db.Queries
}

// NewPostgres creates a new Postgres repository.
func NewPostgres(sqlDB *sql.DB) *Postgres {
	return &Postgres{
		db:      sqlDB,
		queries: db.New(sqlDB),
	}
}

// q returns the queries object, using the transaction from the context if present.
func (r *Postgres) q(ctx context.Context) *db.Queries {
	if tx, ok := db.GetTx(ctx); ok {
		return r.queries.WithTx(tx)
	}
	return r.queries
}

// Save persists a Contest with its options and predictions to the database.
// It handles both creation and updates.
func (r *Postgres) Save(ctx context.Context, c *contest.Contest) error {
	// Check if we are already in a transaction
	tx, hasTx := db.GetTx(ctx)
	if !hasTx {
		// No external transaction, start one
		var err error
		tx, err = r.db.BeginTx(ctx, nil)
		if err != nil {
			return fmt.Errorf("failed to begin transaction: %w", err)
		}
		defer tx.Rollback()
	}

	qtx := r.queries.WithTx(tx)

	// Check if contest exists (ID will be 0 for new contests)
	isNew := c.ID == 0
	var err error

	if isNew {
		// Create new contest
		result, err := qtx.CreateContest(ctx, db.CreateContestParams{
			CircleID:  int32(c.CircleID),
			CreatorID: uuid.UUID(c.CreatorID),
			Question:  c.Question,
			Status:    string(c.Status),
			MinStake:  int32(c.MinStake),
			HouseRake: c.HouseRake,
			CreatedAt: c.CreatedAt,
			LockedAt:  c.LockedAt,
			ExpiresAt: c.ExpiresAt,
			Duration:  c.Duration,
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
		resultOptionID := sql.NullInt32{Valid: false}
		if c.ResultOptionID != nil {
			resultOptionID = sql.NullInt32{Int32: int32(*c.ResultOptionID), Valid: true}
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

	for _, prediction := range c.Predictions {
		err = qtx.UpsertPrediction(ctx, db.UpsertPredictionParams{
			ContestID: int32(c.ID),
			UserID:    uuid.UUID(prediction.UserID),
			OptionID:  int32(prediction.OptionID),
			Clout:     int32(prediction.Clout),
			CreatedAt: prediction.Timestamp,
		})
		if err != nil {
			return fmt.Errorf("failed to save prediction: %w", err)
		}
	}

	if !hasTx {
		if err := tx.Commit(); err != nil {
			return fmt.Errorf("failed to commit transaction: %w", err)
		}
	}

	return nil
}

// FindByID retrieves a Contest with all its options and predictions by ID.
func (r *Postgres) FindByID(ctx context.Context, id contest.ID) (*contest.Contest, error) {
	dbContest, err := r.q(ctx).GetContest(ctx, int32(id))
	if err != nil {
		return nil, fmt.Errorf("failed to find contest by id: %w", err)
	}

	return r.mapContest(ctx, dbContest)
}

// FindByCircleID retrieves all Contests for a given Circle.
func (r *Postgres) FindByCircleID(ctx context.Context, circleID circle.ID) ([]*contest.Contest, error) {
	dbContests, err := r.q(ctx).ListContestsByCircle(ctx, int32(circleID))
	if err != nil {
		return nil, fmt.Errorf("failed to find contests by circle id: %w", err)
	}

	contests := make([]*contest.Contest, len(dbContests))
	for i, dbContest := range dbContests {
		c, err := r.mapContest(ctx, dbContest)
		if err != nil {
			return nil, err
		}
		contests[i] = c
	}

	return contests, nil
}

// FindContestsToLock retrieves all contests that are OPEN and have passed their locked_at time.
func (r *Postgres) FindContestsToLock(ctx context.Context) ([]*contest.Contest, error) {
	dbContests, err := r.q(ctx).FindContestsToLock(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to find contests to lock: %w", err)
	}

	contests := make([]*contest.Contest, len(dbContests))
	for i, dbContest := range dbContests {
		c, err := r.mapContest(ctx, dbContest)
		if err != nil {
			return nil, err
		}
		contests[i] = c
	}

	return contests, nil
}

// FindContestsToExpire retrieves all contests that are OPEN or LOCKED and have passed their expires_at time.
func (r *Postgres) FindContestsToExpire(ctx context.Context) ([]*contest.Contest, error) {
	dbContests, err := r.q(ctx).FindContestsToExpire(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to find contests to expire: %w", err)
	}

	contests := make([]*contest.Contest, len(dbContests))
	for i, dbContest := range dbContests {
		c, err := r.mapContest(ctx, dbContest)
		if err != nil {
			return nil, err
		}
		contests[i] = c
	}

	return contests, nil
}

func (r *Postgres) mapContest(ctx context.Context, dbContest db.Contest) (*contest.Contest, error) {
	id := dbContest.ID

	// Load options
	dbOptions, err := r.q(ctx).ListContestOptions(ctx, int32(id))
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
	dbPredictions, err := r.q(ctx).ListContestPredictions(ctx, int32(id))
	if err != nil {
		return nil, fmt.Errorf("failed to load contest predictions: %w", err)
	}

	predictions := make([]*contest.Prediction, len(dbPredictions))
	for i, dbPrediction := range dbPredictions {
		predictions[i] = &contest.Prediction{
			UserID:    user.ID(dbPrediction.UserID),
			OptionID:  int(dbPrediction.OptionID),
			Clout:     int(dbPrediction.Clout),
			Timestamp: dbPrediction.CreatedAt,
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

	houseRake := dbContest.HouseRake

	return &contest.Contest{
		ID:             contest.ID(dbContest.ID),
		CircleID:       circle.ID(dbContest.CircleID),
		CreatorID:      user.ID(dbContest.CreatorID),
		Question:       dbContest.Question,
		Options:        options,
		Predictions:    predictions,
		Status:         contest.Status(dbContest.Status),
		MinStake:       minStake,
		HouseRake:      houseRake,
		ResultOptionID: resultOptionID,
		CreatedAt:      dbContest.CreatedAt,
		LockedAt:       dbContest.LockedAt,
		ExpiresAt:      dbContest.ExpiresAt,
		Duration:       dbContest.Duration,
	}, nil
}

// UpdateStatus updates the status of a contest.
func (r *Postgres) UpdateStatus(ctx context.Context, id contest.ID, status contest.Status) error {
	err := r.q(ctx).UpdateContestStatusOnly(ctx, db.UpdateContestStatusOnlyParams{
		ID:     int32(id),
		Status: string(status),
	})
	if err != nil {
		return fmt.Errorf("failed to update contest status: %w", err)
	}
	return nil
}
