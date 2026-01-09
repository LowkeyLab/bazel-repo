package service_test

import (
	"context"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/clock"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	contestrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCircleService(t *testing.T) {
	testutil.WithTestDB(t, func(t *testing.T, pool *pgxpool.Pool) {
		ctx := context.Background()

		setup := func(t *testing.T) (*service.Service, *repository.Postgres, *db.Queries) {
			testutil.ResetTables(t, pool)

			repo := repository.NewPostgres(pool)
			contestRepo := contestrepo.NewPostgres(pool)
			clk := clock.RealClock{}
			authClient := &MockAuthorizerClient{}
			svc := service.NewService(repo, contestRepo, clk, authClient)
			queries := db.New(pool)
			return svc, repo, queries
		}

		t.Run("CreateCircle", func(t *testing.T) {
			svc, _, q := setup(t)

			creatorID := user.ID("alice-id")

			c, err := svc.CreateCircle(ctx, "Book Club", creatorID)
			require.NoError(t, err)
			assert.NotNil(t, c)
			assert.Equal(t, "Book Club", c.Name)
			assert.Len(t, c.Members, 1)

			dbCircle, err := q.GetCircle(ctx, int32(c.ID))
			require.NoError(t, err)
			assert.Equal(t, "Book Club", dbCircle.Name)

			dbMember, err := q.GetCircleMember(ctx, db.GetCircleMemberParams{
				CircleID: int32(c.ID),
				UserID:   string(creatorID),
			})
			require.NoError(t, err)
			assert.Equal(t, int32(1000), dbMember.Clout)
		})

		t.Run("CreateCircle_WithEmptyName", func(t *testing.T) {
			svc, _, _ := setup(t)

			creatorID := user.ID("bob-id")

			c, err := svc.CreateCircle(ctx, "", creatorID)
			assert.Error(t, err)
			assert.Nil(t, c)
		})

		t.Run("DeleteCircle_ByCreator", func(t *testing.T) {
			svc, _, q := setup(t)

			creatorID := user.ID("creator-id")
			circleObj, err := svc.CreateCircle(ctx, "Gamers", creatorID)
			require.NoError(t, err)

			require.NoError(t, svc.DeleteCircle(ctx, circleObj.ID, creatorID))

			_, err = q.GetCircle(ctx, int32(circleObj.ID))
			assert.Error(t, err)
		})

		t.Run("ListUserCircles", func(t *testing.T) {
			svc, _, _ := setup(t)

			user1ID := user.ID("user1-id")
			user2ID := user.ID("user2-id")

			circle1, err := svc.CreateCircle(ctx, "Circle 1", user1ID)
			require.NoError(t, err)

			_, err = svc.CreateCircle(ctx, "Circle 2", user1ID)
			require.NoError(t, err)

			_, err = svc.CreateCircle(ctx, "Circle 3", user2ID)
			require.NoError(t, err)

			err = svc.AddMember(ctx, circle1.ID, user2ID)
			require.NoError(t, err)

			circles1, err := svc.ListUserCircles(ctx, user1ID)
			require.NoError(t, err)
			assert.Len(t, circles1, 2)

			circles2, err := svc.ListUserCircles(ctx, user2ID)
			require.NoError(t, err)
			assert.Len(t, circles2, 2)
		})

		t.Run("Predict_AllowsUpdatingExistingStake", func(t *testing.T) {
			svc, _, q := setup(t)
			creatorID := user.ID("creator")
			memberID := user.ID("predictor")

			circleObj, err := svc.CreateCircle(ctx, "Prediction Circle", creatorID)
			require.NoError(t, err)

			require.NoError(t, svc.AddMember(ctx, circleObj.ID, memberID))

			contestObj, err := svc.CreateContest(ctx, circleObj.ID, creatorID, "Who wins?", []string{"Yes", "No"}, contest.Duration1Day, 100)
			require.NoError(t, err)

			require.NoError(t, svc.Predict(ctx, circleObj.ID, contestObj.ID, memberID, 1, 200))

			memberAfterFirst, err := q.GetCircleMember(ctx, db.GetCircleMemberParams{
				CircleID: int32(circleObj.ID),
				UserID:   string(memberID),
			})
			require.NoError(t, err)
			assert.Equal(t, int32(800), memberAfterFirst.Clout)

			require.NoError(t, svc.Predict(ctx, circleObj.ID, contestObj.ID, memberID, 1, 120))

			memberAfterSecond, err := q.GetCircleMember(ctx, db.GetCircleMemberParams{
				CircleID: int32(circleObj.ID),
				UserID:   string(memberID),
			})
			require.NoError(t, err)
			assert.Equal(t, int32(880), memberAfterSecond.Clout)
		})
	})
}
