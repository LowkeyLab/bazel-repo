package service_test

import (
	"context"
	"database/sql"
	"testing"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/sse"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	contestrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Deliver through the real SSE subscription and inspect its consumer before
// returning to the publisher, so an early notification cannot race past commit.
type settlementSubscriber struct {
	*sse.Manager
	consume func()
}

func (s *settlementSubscriber) OnEvent(event circle.DomainEvent) {
	s.Manager.OnEvent(event)
	if s.consume != nil {
		s.consume()
	}
}

func TestSettlement(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, sqlDB *sql.DB) {
		for _, scenario := range []string{"statement failure after first winner credit", "deferred commit failure", "disconnected subscriber", "full subscriber buffer"} {
			t.Run(scenario, func(t *testing.T) {
				testutil.ResetTables(t, sqlDB)
				ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
				defer cancel()
				events := &settlementSubscriber{Manager: sse.NewManager()}
				newService := func() *service.Service {
					return service.NewService(repository.NewPostgres(sqlDB), userrepo.NewPostgres(sqlDB), contestrepo.NewPostgres(sqlDB), clock.RealClock{}, &TestTxManager{sqlDB: sqlDB}, events)
				}
				svc := newService()
				creator := createTestUser(t, sqlDB, "creator")
				winnerA := createTestUser(t, sqlDB, "winner_a")
				winnerB := createTestUser(t, sqlDB, "winner_b")
				loser := createTestUser(t, sqlDB, "loser")
				circ, err := svc.CreateCircle(ctx, "Settlement", creator)
				require.NoError(t, err)
				for _, id := range []user.ID{winnerA, winnerB, loser} {
					require.NoError(t, svc.AddMember(ctx, circ.ID, id))
				}
				cont, err := svc.CreateContest(ctx, circ.ID, creator, "Who wins?", []string{"A", "B"}, contest.Duration1Day, 10)
				require.NoError(t, err)
				require.NoError(t, svc.Predict(ctx, circ.ID, cont.ID, winnerA, 1, 100))
				require.NoError(t, svc.Predict(ctx, circ.ID, cont.ID, winnerB, 1, 100))
				require.NoError(t, svc.Predict(ctx, circ.ID, cont.ID, loser, 2, 200))
				before, err := svc.GetContest(ctx, cont.ID)
				require.NoError(t, err)
				notifications, unsubscribe := events.Subscribe(cont.ID)
				defer func() { unsubscribe() }()
				reader := newService()
				assertBalances := func(want map[user.ID]int) {
					t.Helper()
					got, err := reader.GetCircleWithUsernames(ctx, circ.ID)
					require.NoError(t, err)
					balances := make(map[user.ID]int)
					for _, m := range got.Members {
						balances[m.UserID] = m.Clout
					}
					assert.Equal(t, want, balances)
				}
				assertSettled := func() {
					t.Helper()
					// Two 100 stakes share the losing 200 minus 20 rake: each gets 190.
					assertBalances(map[user.ID]int{creator: 1000, winnerA: 1090, winnerB: 1090, loser: 800})
					settled, err := reader.GetContest(ctx, cont.ID)
					require.NoError(t, err)
					assert.Equal(t, contest.StatusResolved, settled.Status)
					require.NotNil(t, settled.ResultOptionID)
					assert.Equal(t, 1, *settled.ResultOptionID)
				}
				if scenario == "disconnected subscriber" {
					unsubscribe()
					// Unsubscribe only once; the deferred cleanup below is replaced by a no-op.
					unsubscribe = func() {}
					require.NoError(t, svc.ResolveAndDistributeContestClout(ctx, circ.ID, cont.ID, creator, 1))
					assertSettled()
					return
				}
				if scenario == "full subscriber buffer" {
					for range cap(notifications) {
						events.Broadcast(cont.ID, sse.Event{Type: sse.EventContestUpdated})
					}
					require.NoError(t, svc.ResolveAndDistributeContestClout(ctx, circ.ID, cont.ID, creator, 1))
					assertSettled()
					return
				}
				// Sequences survive rollback, proving both updates were reached regardless
				// of map iteration order. Install the fault only after prediction setup.
				fault := `
CREATE SEQUENCE settlement_credit_attempt;
CREATE FUNCTION record_credit() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF nextval('settlement_credit_attempt') = 2 THEN
  RAISE EXCEPTION 'injected second credit failure';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER settlement_fault AFTER UPDATE OF clout ON circle_members
 FOR EACH ROW EXECUTE FUNCTION record_credit();`
				wantError := "injected second credit failure"
				if scenario == "deferred commit failure" {
					fault = `
CREATE SEQUENCE settlement_credit_attempt;
CREATE FUNCTION record_credit() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM nextval('settlement_credit_attempt');
 RETURN NEW;
END $$;
CREATE TRIGGER settlement_fault AFTER UPDATE OF clout ON circle_members
 FOR EACH ROW EXECUTE FUNCTION record_credit();
CREATE FUNCTION fail_settlement_commit() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (SELECT last_value FROM settlement_credit_attempt) <> 2 THEN
  RAISE EXCEPTION 'both credits must execute before commit';
 END IF;
 RAISE EXCEPTION 'injected deferred commit failure';
END $$;
CREATE CONSTRAINT TRIGGER settlement_commit_fault AFTER UPDATE ON circle_members
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fail_settlement_commit();`
					wantError = "injected deferred commit failure"
				}
				_, err = sqlDB.ExecContext(ctx, fault)
				require.NoError(t, err)
				removeFault := func() {
					t.Helper()
					_, err := sqlDB.Exec(`DROP TRIGGER IF EXISTS settlement_fault ON circle_members;
DROP TRIGGER IF EXISTS settlement_commit_fault ON circle_members;
DROP FUNCTION IF EXISTS record_credit(); DROP FUNCTION IF EXISTS fail_settlement_commit();
DROP SEQUENCE IF EXISTS settlement_credit_attempt;`)
					require.NoError(t, err)
				}
				t.Cleanup(removeFault)
				err = svc.ResolveAndDistributeContestClout(ctx, circ.ID, cont.ID, creator, 1)
				require.ErrorContains(t, err, wantError)
				var attempts int
				require.NoError(t, sqlDB.QueryRowContext(ctx, `SELECT last_value FROM settlement_credit_attempt`).Scan(&attempts))
				assert.Equal(t, 2, attempts, "both winner updates reached the database")
				after, err := reader.GetContest(ctx, cont.ID)
				require.NoError(t, err)
				assert.Equal(t, before, after, "failed settlement preserves contest and predictions")
				assertBalances(map[user.ID]int{creator: 1000, winnerA: 900, winnerB: 900, loser: 800})
				select {
				case event := <-notifications:
					t.Errorf("rolled back settlement notified consumer: %+v", event)
				default:
				}
				removeFault()
				observed := false
				events.consume = func() {
					select {
					case event := <-notifications:
						assert.Equal(t, sse.EventContestUpdated, event.Type)
						// This reads outside the transaction context, through a fresh service and
						// a separate pool connection while notification delivery is still active.
						assertSettled()
						observed = true
					case <-ctx.Done():
						t.Error("no settlement notification", ctx.Err())
					}
				}
				require.NoError(t, svc.ResolveAndDistributeContestClout(ctx, circ.ID, cont.ID, creator, 1))
				assert.True(t, observed, "consumer must receive successful settlement")
				assertSettled()
			})
		}
	})
}
