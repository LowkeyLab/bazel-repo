package circle

import (
	"errors"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// ID represents the unique identifier for a Circle.
type ID int32

// Circle represents a private group of friends.
type Circle struct {
	ID        ID
	Name      string
	Members   map[user.ID]*Member // Keyed by User ID
	CreatedAt time.Time
}

// Member represents a user within a specific Circle.
type Member struct {
	UserID user.ID
	Clout  int
}

// New creates a new Circle with the creator as the first member.
func New(name string, creatorID user.ID) (*Circle, error) {
	if name == "" {
		return nil, errors.New("circle name cannot be empty")
	}

	c := &Circle{
		ID:        0, // ID will be set by database
		Name:      name,
		Members:   make(map[user.ID]*Member),
		CreatedAt: time.Now(),
	}

	c.AddMember(creatorID)
	return c, nil
}

// AddMember adds a user to the circle with an initial balance.
func (c *Circle) AddMember(userID user.ID) {
	if _, exists := c.Members[userID]; !exists {
		c.Members[userID] = &Member{
			UserID: userID,
			Clout:  1000, // Initial Clout
		}
	}
}
