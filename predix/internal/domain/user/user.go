package user

// ID represents the unique identifier for a User.
type ID int32

// User represents a user of the Predix platform.
type User struct {
	ID    ID
	Name  string
	Email string
}

// New creates a new User without an ID (will be assigned by database).
func New(name, email string) (*User, error) {
	return &User{
		ID:    0, // ID will be set by database
		Name:  name,
		Email: email,
	}, nil
}
