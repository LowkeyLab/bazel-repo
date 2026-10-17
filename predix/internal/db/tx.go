package db

import (
	"context"
	"database/sql"
	"fmt"
)

type txKey struct{}

// RunInTx executes the given function within a database transaction.
func RunInTx(ctx context.Context, db *sql.DB, fn func(ctx context.Context) error) error {
	tx, err := db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()

	// Store the transaction in the context
	ctxWithTx := context.WithValue(ctx, txKey{}, tx)

	if err := fn(ctxWithTx); err != nil {
		return err
	}

	return tx.Commit()
}

// GetTx retrieves the transaction from the context, if it exists.
func GetTx(ctx context.Context) (*sql.Tx, bool) {
	tx, ok := ctx.Value(txKey{}).(*sql.Tx)
	return tx, ok
}

// RunInCircleTx serializes transactions for the same circle before reading state.
// The row lock lasts through commit or rollback; different circles remain independent.
func RunInCircleTx(ctx context.Context, sqlDB *sql.DB, circleID int32, fn func(context.Context) error) error {
	return RunInTx(ctx, sqlDB, func(ctx context.Context) error {
		tx, _ := GetTx(ctx)
		if err := New(tx).LockCircleForSettlement(ctx, circleID); err != nil {
			return fmt.Errorf("lock circle for settlement: %w", err)
		}
		return fn(ctx)
	})
}
