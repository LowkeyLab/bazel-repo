package service_test

import (
	"context"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/lowkeylab/bazel-repo/predix/internal/db"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	userrepo "github.com/lowkeylab/bazel-repo/predix/internal/domain/user/repository"
	"github.com/lowkeylab/bazel-repo/predix/internal/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// createTestUser creates a test user in the database
func createTestUser(t *testing.T, pool *pgxpool.Pool, username string) user.ID {
	return createTestUserWithRole(t, pool, username, db.UserRoleMember)
}

func createTestUserWithRole(t *testing.T, pool *pgxpool.Pool, username string, role db.UserRole) user.ID {
	t.Helper()

	ctx := context.Background()
	q := db.New(pool)
	passwordHash := "hash"

	result, err := q.CreateUser(ctx, db.CreateUserParams{
		Username:     username,
		PasswordHash: passwordHash,
		Role:         role,
	})
	require.NoError(t, err)

	return user.ID(result.ID)
}

func TestCreateCircle(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)
	q := db.New(pool)

	// Create a user to be the circle creator
	creatorID := createTestUser(t, pool, "alice")

	// Test: Create a circle
	c, err := svc.CreateCircle(ctx, "Book Club", creatorID)
	require.NoError(t, err)
	assert.NotNil(t, c)
	assert.Equal(t, "Book Club", c.Name)
	assert.Len(t, c.Members, 1)

	// Verify in database
	dbCircle, err := q.GetCircle(ctx, int32(c.ID))
	require.NoError(t, err)
	assert.Equal(t, "Book Club", dbCircle.Name)

	// Verify creator is a member with initial clout
	dbMember, err := q.GetCircleMember(ctx, db.GetCircleMemberParams{
		CircleID: int32(c.ID),
		UserID:   int32(creatorID),
	})
	require.NoError(t, err)
	assert.Equal(t, int32(1000), dbMember.Clout)
}

func TestCreateCircle_WithEmptyName(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)

	creatorID := createTestUser(t, pool, "bob")

	// Test: Create circle with empty name should fail
	c, err := svc.CreateCircle(ctx, "", creatorID)
	assert.Error(t, err)
	assert.Nil(t, c)
}

func TestDeleteCircle_ByCreator(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)
	q := db.New(pool)

	creatorID := createTestUser(t, pool, "creator")
	circleObj, err := svc.CreateCircle(ctx, "Gamers", creatorID)
	require.NoError(t, err)

	require.NoError(t, svc.DeleteCircle(ctx, circleObj.ID, creatorID))

	_, err = q.GetCircle(ctx, int32(circleObj.ID))
	assert.ErrorIs(t, err, pgx.ErrNoRows)
}

func TestDeleteCircle_ByAdmin(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)
	q := db.New(pool)

	creatorID := createTestUser(t, pool, "creator")
	adminID := createTestUserWithRole(t, pool, "admin", db.UserRoleAdmin)

	circleObj, err := svc.CreateCircle(ctx, "Readers", creatorID)
	require.NoError(t, err)

	require.NoError(t, svc.DeleteCircle(ctx, circleObj.ID, adminID))
	_, err = q.GetCircle(ctx, int32(circleObj.ID))
	assert.ErrorIs(t, err, pgx.ErrNoRows)
}

func TestDeleteCircle_ForbiddenForMember(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)

	creatorID := createTestUser(t, pool, "creator")
	memberID := createTestUser(t, pool, "member")

	circleObj, err := svc.CreateCircle(ctx, "Runners", creatorID)
	require.NoError(t, err)

	err = svc.DeleteCircle(ctx, circleObj.ID, memberID)
	assert.ErrorIs(t, err, service.ErrNotCircleOwner)

	// Circle should still exist
	_, err = repo.FindByID(ctx, circleObj.ID)
	require.NoError(t, err)
}

func TestListUserCircles(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)

	// Create users
	user1ID := createTestUser(t, pool, "user1")
	user2ID := createTestUser(t, pool, "user2")

	// Create circles
	circle1, err := svc.CreateCircle(ctx, "Circle 1", user1ID)
	require.NoError(t, err)

	circle2, err := svc.CreateCircle(ctx, "Circle 2", user1ID)
	require.NoError(t, err)

	circle3, err := svc.CreateCircle(ctx, "Circle 3", user2ID)
	require.NoError(t, err)

	// Add user2 to circle1
	err = svc.AddMember(ctx, circle1.ID, user2ID)
	require.NoError(t, err)

	// User1 should have 2 circles
	circles1, err := svc.ListUserCircles(ctx, user1ID)
	require.NoError(t, err)
	assert.Len(t, circles1, 2)
	circleIDs1 := map[int32]bool{}
	for _, c := range circles1 {
		circleIDs1[int32(c.ID)] = true
	}
	assert.True(t, circleIDs1[int32(circle1.ID)])
	assert.True(t, circleIDs1[int32(circle2.ID)])

	// User2 should have 2 circles (circle3 and circle1 where added as member)
	circles2, err := svc.ListUserCircles(ctx, user2ID)
	require.NoError(t, err)
	assert.Len(t, circles2, 2)
	circleIDs2 := map[int32]bool{}
	for _, c := range circles2 {
		circleIDs2[int32(c.ID)] = true
	}
	assert.True(t, circleIDs2[int32(circle1.ID)])
	assert.True(t, circleIDs2[int32(circle3.ID)])
}

func TestListUserCircles_EmptyList(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)

	userID := createTestUser(t, pool, "lonely_user")

	// User with no circles should return empty list
	circles, err := svc.ListUserCircles(ctx, userID)
	require.NoError(t, err)
	assert.Len(t, circles, 0)
}

func TestGetCircleWithUsernames(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)

	// Create users
	creatorID := createTestUser(t, pool, "creator_alice")
	member1ID := createTestUser(t, pool, "member_bob")
	member2ID := createTestUser(t, pool, "member_charlie")

	// Create circle
	circle, err := svc.CreateCircle(ctx, "Movie Club", creatorID)
	require.NoError(t, err)

	// Add members
	err = svc.AddMember(ctx, circle.ID, member1ID)
	require.NoError(t, err)
	err = svc.AddMember(ctx, circle.ID, member2ID)
	require.NoError(t, err)

	// Get enriched circle
	enriched, err := svc.GetCircleWithUsernames(ctx, circle.ID)
	require.NoError(t, err)
	assert.NotNil(t, enriched)
	assert.Equal(t, "Movie Club", enriched.Name)
	assert.Equal(t, circle.ID, enriched.ID)
	assert.Equal(t, creatorID, enriched.CreatorID)
	assert.Len(t, enriched.Members, 3)

	// Verify all members have usernames
	usernames := make(map[string]bool)
	for _, m := range enriched.Members {
		assert.NotEmpty(t, m.Username, "member %d should have username", m.UserID)
		assert.Equal(t, 1000, m.Clout, "member should have initial clout")
		usernames[m.Username] = true
	}

	assert.True(t, usernames["creator_alice"])
	assert.True(t, usernames["member_bob"])
	assert.True(t, usernames["member_charlie"])
}

func TestGetCircleWithUsernames_CircleNotFound(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)

	// Try to get non-existent circle
	enriched, err := svc.GetCircleWithUsernames(ctx, 99999)
	assert.Error(t, err)
	assert.Nil(t, enriched)
}

func TestListUserCirclesWithUsernames(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)

	// Create users
	user1ID := createTestUser(t, pool, "user_one")
	user2ID := createTestUser(t, pool, "user_two")
	user3ID := createTestUser(t, pool, "user_three")

	// Create circles
	circle1, err := svc.CreateCircle(ctx, "Tech Circle", user1ID)
	require.NoError(t, err)

	circle2, err := svc.CreateCircle(ctx, "Sports Circle", user1ID)
	require.NoError(t, err)

	// Create a third circle for user2 (to ensure user1 doesn't see it)
	_, err = svc.CreateCircle(ctx, "Art Circle", user2ID)
	require.NoError(t, err)

	// Add user2 to circle1
	err = svc.AddMember(ctx, circle1.ID, user2ID)
	require.NoError(t, err)

	// Add user3 to circle2
	err = svc.AddMember(ctx, circle2.ID, user3ID)
	require.NoError(t, err)

	// Get enriched circles for user1
	enrichedCircles, err := svc.ListUserCirclesWithUsernames(ctx, user1ID)
	require.NoError(t, err)
	assert.Len(t, enrichedCircles, 2)

	// Verify each circle has enriched member data
	for _, ec := range enrichedCircles {
		assert.NotEmpty(t, ec.Name)
		assert.NotEmpty(t, ec.Members)

		// All members should have usernames
		for _, m := range ec.Members {
			assert.NotEmpty(t, m.Username, "member %d in circle %s should have username", m.UserID, ec.Name)
			assert.Greater(t, m.Clout, 0)
		}
	}

	// Find Tech Circle and verify members
	var techCircle *service.EnrichedCircle
	for _, ec := range enrichedCircles {
		if ec.Name == "Tech Circle" {
			techCircle = ec
			break
		}
	}
	require.NotNil(t, techCircle)
	assert.Len(t, techCircle.Members, 2) // user1 (creator) and user2 (added)

	// Verify usernames are present
	usernames := make(map[string]bool)
	for _, m := range techCircle.Members {
		usernames[m.Username] = true
	}
	assert.True(t, usernames["user_one"])
	assert.True(t, usernames["user_two"])

	// Find Sports Circle and verify members
	var sportsCircle *service.EnrichedCircle
	for _, ec := range enrichedCircles {
		if ec.Name == "Sports Circle" {
			sportsCircle = ec
			break
		}
	}
	require.NotNil(t, sportsCircle)
	assert.Len(t, sportsCircle.Members, 2) // user1 (creator) and user3 (added)

	usernames = make(map[string]bool)
	for _, m := range sportsCircle.Members {
		usernames[m.Username] = true
	}
	assert.True(t, usernames["user_one"])
	assert.True(t, usernames["user_three"])
}

func TestListUserCirclesWithUsernames_EmptyList(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)

	userID := createTestUser(t, pool, "isolated_user")

	// User with no circles should return empty list
	enrichedCircles, err := svc.ListUserCirclesWithUsernames(ctx, userID)
	require.NoError(t, err)
	assert.Len(t, enrichedCircles, 0)
}

func TestListUserCirclesWithUsernames_MultipleMembers(t *testing.T) {
	pool := testutil.SetupTestDB(t)

	ctx := context.Background()
	repo := repository.NewPostgres(pool)
	userRepo := userrepo.NewPostgres(pool)
	svc := service.NewService(repo, userRepo)

	// Create users
	creatorID := createTestUser(t, pool, "circle_creator")
	member1ID := createTestUser(t, pool, "member_alpha")
	member2ID := createTestUser(t, pool, "member_beta")
	member3ID := createTestUser(t, pool, "member_gamma")

	// Create circle with multiple members
	circle, err := svc.CreateCircle(ctx, "Big Circle", creatorID)
	require.NoError(t, err)

	err = svc.AddMember(ctx, circle.ID, member1ID)
	require.NoError(t, err)
	err = svc.AddMember(ctx, circle.ID, member2ID)
	require.NoError(t, err)
	err = svc.AddMember(ctx, circle.ID, member3ID)
	require.NoError(t, err)

	// Get enriched circles for creator
	enrichedCircles, err := svc.ListUserCirclesWithUsernames(ctx, creatorID)
	require.NoError(t, err)
	assert.Len(t, enrichedCircles, 1)

	enriched := enrichedCircles[0]
	assert.Equal(t, "Big Circle", enriched.Name)
	assert.Len(t, enriched.Members, 4)

	// Verify all usernames are present and correct
	usernames := make(map[string]user.ID)
	for _, m := range enriched.Members {
		usernames[m.Username] = m.UserID
	}

	assert.Equal(t, creatorID, usernames["circle_creator"])
	assert.Equal(t, member1ID, usernames["member_alpha"])
	assert.Equal(t, member2ID, usernames["member_beta"])
	assert.Equal(t, member3ID, usernames["member_gamma"])
}
