package service_test

import (
	"context"
	"database/sql"
	"testing"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
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

func TestConcurrentSettlement(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, sqlDB *sql.DB) {
		for _, scenario := range []string{"same contest", "shared winner"} {
			t.Run(scenario, func(t *testing.T) {
				testutil.ResetTables(t, sqlDB)
				ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
				defer cancel()
				newService := func() *service.Service {
					return service.NewService(repository.NewPostgres(sqlDB), userrepo.NewPostgres(sqlDB), contestrepo.NewPostgres(sqlDB), clock.RealClock{}, &TestTxManager{sqlDB: sqlDB}, sse.NewManager())
				}
				svc := newService()
				creator := createTestUser(t, sqlDB, "creator")
				winner := createTestUser(t, sqlDB, "winner")
				loser := createTestUser(t, sqlDB, "loser")
				circ, err := svc.CreateCircle(ctx, "Concurrent settlement", creator)
				require.NoError(t, err)
				for _, id := range []user.ID{winner, loser} {
					require.NoError(t, svc.AddMember(ctx, circ.ID, id))
				}
				makeContest := func(stake int) contest.ID {
					t.Helper()
					cont, err := svc.CreateContest(ctx, circ.ID, creator, "Who wins?", []string{"A", "B"}, contest.Duration1Day, 10)
					require.NoError(t, err)
					require.NoError(t, svc.Predict(ctx, circ.ID, cont.ID, winner, 1, stake))
					require.NoError(t, svc.Predict(ctx, circ.ID, cont.ID, loser, 2, stake))
					return cont.ID
				}
				first := makeContest(100)
				second := first
				if scenario == "shared winner" {
					second = makeContest(200)
				}

				// Hold a real database write until both requests are waiting on locks.
				// With serialization, the second request instead waits behind the first.
				gate, err := sqlDB.BeginTx(ctx, nil)
				require.NoError(t, err)
				defer gate.Rollback()
				if scenario == "same contest" {
					_, err = gate.ExecContext(ctx, "SELECT id FROM contests WHERE id = $1 FOR UPDATE", first)
				} else {
					_, err = gate.ExecContext(ctx, "SELECT user_id FROM circle_members WHERE circle_id = $1 AND user_id = $2 FOR UPDATE", circ.ID, winner)
				}
				require.NoError(t, err)
				results := make(chan error, 2)
				for _, id := range []contest.ID{first, second} {
					go func() { results <- newService().ResolveAndDistributeContestClout(ctx, circ.ID, id, creator, 1) }()
				}
				for {
					var waiting int
					err := sqlDB.QueryRowContext(ctx, `SELECT count(*) FROM pg_stat_activity
WHERE datname = current_database() AND wait_event_type = 'Lock' AND pid <> pg_backend_pid()`).Scan(&waiting)
					require.NoError(t, err, "both settlement requests must overlap before releasing the gate")
					if waiting >= 2 {
						break
					}
				}
				require.NoError(t, gate.Rollback())
				successes := 0
				for range 2 {
					select {
					case err := <-results:
						if err == nil {
							successes++
						} else if scenario == "same contest" {
							assert.ErrorContains(t, err, "contest is already resolved")
						} else {
							assert.NoError(t, err)
						}
					case <-ctx.Done():
						t.Fatal("settlement did not finish", ctx.Err())
					}
				}
				wantWinner, wantLoser := 1090, 900
				if scenario == "same contest" {
					assert.Equal(t, 1, successes)
				} else {
					assert.Equal(t, 2, successes)
					// Stakes total 300; payouts 190 + 380 leave winner 1270, loser 700.
					wantWinner, wantLoser = 1270, 700
				}
				reader := newService()
				got, err := reader.GetCircleWithUsernames(ctx, circ.ID)
				require.NoError(t, err)
				balances := make(map[user.ID]int)
				for _, member := range got.Members {
					balances[member.UserID] = member.Clout
				}
				assert.Equal(t, map[user.ID]int{creator: 1000, winner: wantWinner, loser: wantLoser}, balances)
				for _, id := range []contest.ID{first, second} {
					settled, err := reader.GetContest(ctx, id)
					require.NoError(t, err)
					assert.Equal(t, contest.StatusResolved, settled.Status)
					require.NotNil(t, settled.ResultOptionID)
					assert.Equal(t, 1, *settled.ResultOptionID)
				}
			})
		}
	})
}
