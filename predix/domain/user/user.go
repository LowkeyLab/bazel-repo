package user

import "github.com/google/uuid"

// ID represents the unique identifier for a User.
type ID uuid.UUID

// User represents a user of the Predix platform.
type User struct {
	ID    ID
	Name  string
	Email string
}

// New creates a new User.
func New(name, email string) (*User, error) {
	id, err := uuid.NewRandom()
	if err != nil {
		return nil, err
	}
	return &User{
		ID:    ID(id),
		Name:  name,
		Email: email,
	}, nil
}

func (id ID) String() string {
	return uuid.UUID(id).String()
}
