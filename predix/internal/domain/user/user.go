package user

// Role represents the permission level of a User.
type Role string

const (
	RoleMember Role = "member"
	RoleAdmin  Role = "admin"
)

// ID represents the unique identifier for a User.
type ID int32

// User represents a user of the Predix platform.
type User struct {
	ID           ID
	Username     string
	PasswordHash string
	Role         Role
}

// New creates a new User without an ID (will be assigned by database).
func New(username, passwordHash string) (*User, error) {
	return NewWithRole(username, passwordHash, RoleMember)
}

// NewWithRole creates a new User with the specified role.
func NewWithRole(username, passwordHash string, role Role) (*User, error) {
	if role == "" {
		role = RoleMember
	}

	return &User{
		ID:           0, // ID will be set by database
		Username:     username,
		PasswordHash: passwordHash,
		Role:         role,
	}, nil
}
