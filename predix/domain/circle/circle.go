package circle

import (
	"errors"
	"time"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/domain/user"
)

// ID represents the unique identifier for a Circle.
type ID uuid.UUID

// Circle represents a private group of friends.
type Circle struct {
	ID         ID
	Name       string
	InviteCode string
	Members    map[user.ID]*Member // Keyed by User ID
	CreatedAt  time.Time
}

// Member represents a user within a specific Circle.
type Member struct {
	UserID  user.ID
	Balance int // Clout
}

// New creates a new Circle with the creator as the first member.
func New(name string, creatorID user.ID) (*Circle, error) {
	if name == "" {
		return nil, errors.New("circle name cannot be empty")
	}

	id, err := uuid.NewRandom()
	if err != nil {
		return nil, err
	}

	c := &Circle{
		ID:         ID(id),
		Name:       name,
		InviteCode: generateInviteCode(),
		Members:    make(map[user.ID]*Member),
		CreatedAt:  time.Now(),
	}

	c.AddMember(creatorID)
	return c, nil
}

// AddMember adds a user to the circle with an initial balance.
func (c *Circle) AddMember(userID user.ID) {
	if _, exists := c.Members[userID]; !exists {
		c.Members[userID] = &Member{
			UserID:  userID,
			Balance: 1000, // Initial Clout
		}
	}
}

// generateInviteCode generates a random string for invitations.
// In a real app, this would ensure uniqueness.
func generateInviteCode() string {
	return uuid.New().String()[:8]
}

func (id ID) String() string {
	return uuid.UUID(id).String()
}
