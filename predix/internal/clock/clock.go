package clock

import "time"

// Clock provides time functionality for testing.
type Clock interface {
	Now() time.Time
}

// RealClock uses the system clock.
type RealClock struct{}

// Now returns the current system time.
func (RealClock) Now() time.Time {
	return time.Now()
}

// FixedClock returns a fixed time for testing.
type FixedClock struct {
	Time time.Time
}

// Now returns the fixed time.
func (c FixedClock) Now() time.Time {
	return c.Time
}
