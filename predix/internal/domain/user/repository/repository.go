package repository

import (
	"errors"
)

// ErrNotFound is returned when a user cannot be located.
var ErrNotFound = errors.New("user not found")
