package sse

import (
	"sync"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
)

// EventType represents the type of SSE event.
type EventType string

const (
	EventContestUpdated EventType = "contest_updated"
)

// Event represents a server-sent event.
type Event struct {
	Type    EventType
	Payload any
}

// Manager handles SSE subscriptions and broadcasting.
type Manager struct {
	mu          sync.RWMutex
	subscribers map[contest.ID][]chan Event
}

// NewManager creates a new SSE manager.
func NewManager() *Manager {
	return &Manager{
		subscribers: make(map[contest.ID][]chan Event),
	}
}

// OnEvent handles domain events and broadcasts them to subscribers.
func (m *Manager) OnEvent(event circle.DomainEvent) {
	switch e := event.(type) {
	case contest.ContestUpdated:
		m.Broadcast(e.ContestID, Event{Type: EventContestUpdated})
	}
}

// Subscribe returns a channel that receives events for the given contest.
// The caller must consume from the channel to prevent blocking.
// We return the channel and an unsubscribe function.
func (m *Manager) Subscribe(contestID contest.ID) (chan Event, func()) {
	m.mu.Lock()
	defer m.mu.Unlock()

	// Buffer size of 10 to allow for some bursts without dropping/blocking immediately
	ch := make(chan Event, 10)
	m.subscribers[contestID] = append(m.subscribers[contestID], ch)

	// Unsubscribe function
	return ch, func() {
		m.mu.Lock()
		defer m.mu.Unlock()

		subs := m.subscribers[contestID]
		for i, sub := range subs {
			if sub == ch {
				// Remove the channel
				// Order doesn't matter much, but let's preserve it for now
				m.subscribers[contestID] = append(subs[:i], subs[i+1:]...)
				close(ch)
				break
			}
		}

		if len(m.subscribers[contestID]) == 0 {
			delete(m.subscribers, contestID)
		}
	}
}

// Broadcast sends an event to all subscribers of the given contest.
func (m *Manager) Broadcast(contestID contest.ID, event Event) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	for _, ch := range m.subscribers[contestID] {
		select {
		case ch <- event:
		default:
			// If channel is full, we drop the message to avoid blocking the broadcaster.
			// This prevents slow clients from affecting others.
		}
	}
}
