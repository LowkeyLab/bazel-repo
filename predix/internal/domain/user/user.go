package user

// ID represents the unique identifier for a User (from Authorizer).
type ID string

// User represents a user of the Predix platform.
type User struct {
	ID       ID
	Username string
	Email    string
}

// New creates a new User domain object.
func New(username, id string) (*User, error) {
	return &User{
		ID:       ID(id),
		Username: username,
	}, nil
}
