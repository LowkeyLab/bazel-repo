package circle

import (
	"errors"
	"time"

	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// DomainEvent represents a domain event.
type DomainEvent interface {
	EventName() string
}

// ID represents the unique identifier for a Circle.
type ID int32

// Circle represents a private group of friends.
type Circle struct {
	ID        ID
	Name      string
	CreatorID user.ID
	Members   map[user.ID]*Member // Keyed by User ID
	CreatedAt time.Time
	events    []DomainEvent
}

// Member represents a user within a specific Circle.
type Member struct {
	UserID user.ID
	Clout  int
}

// AddEvent adds a domain event to the circle.
func (c *Circle) AddEvent(event DomainEvent) {
	c.events = append(c.events, event)
}

// PopEvents returns all domain events and clears them.
func (c *Circle) PopEvents() []DomainEvent {
	events := c.events
	c.events = nil
	return events
}

// New creates a new Circle with the creator as the first member.
func New(name string, creatorID user.ID) (*Circle, error) {
	if name == "" {
		return nil, errors.New("circle name cannot be empty")
	}
	if uuid.UUID(creatorID) == uuid.Nil {
		return nil, errors.New("creator id must be valid")
	}

	c := &Circle{
		ID:        0, // ID will be set by database
		Name:      name,
		CreatorID: creatorID,
		Members:   make(map[user.ID]*Member),
		CreatedAt: time.Now(),
	}

	c.AddMember(creatorID)
	return c, nil
}

// MemberAdded event.
type MemberAdded struct {
	CircleID ID
	UserID   user.ID
}

func (e MemberAdded) EventName() string { return "MemberAdded" }

// AddMember adds a user to the circle with an initial balance.
func (c *Circle) AddMember(userID user.ID) {
	if _, exists := c.Members[userID]; !exists {
		c.Members[userID] = &Member{
			UserID: userID,
			Clout:  1000, // Initial Clout
		}
		c.AddEvent(MemberAdded{CircleID: c.ID, UserID: userID})
	}
}
