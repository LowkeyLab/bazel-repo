package sse_test

import (
	"testing"
	"time"

	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/sse"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/stretchr/testify/assert"
)

func TestManager_SubscribeAndBroadcast(t *testing.T) {
	m := sse.NewManager()
	contestID := contest.ID(1)

	// Subscribe
	ch, unsubscribe := m.Subscribe(contestID)
	defer unsubscribe()

	// Broadcast
	event := sse.Event{Type: sse.EventContestUpdated, Payload: "test"}
	m.Broadcast(contestID, event)

	// Receive
	select {
	case received := <-ch:
		assert.Equal(t, event, received)
	case <-time.After(100 * time.Millisecond):
		t.Fatal("timeout waiting for event")
	}
}

func TestManager_MultipleSubscribers(t *testing.T) {
	m := sse.NewManager()
	contestID := contest.ID(1)

	ch1, unsub1 := m.Subscribe(contestID)
	defer unsub1()

	ch2, unsub2 := m.Subscribe(contestID)
	defer unsub2()

	event := sse.Event{Type: sse.EventContestUpdated}
	m.Broadcast(contestID, event)

	// Both should receive
	timeout := time.After(100 * time.Millisecond)
	for i := 0; i < 2; i++ {
		select {
		case <-ch1:
		case <-ch2:
		case <-timeout:
			t.Fatal("timeout waiting for event")
		}
	}
}

func TestManager_Unsubscribe(t *testing.T) {
	m := sse.NewManager()
	contestID := contest.ID(1)

	ch, unsubscribe := m.Subscribe(contestID)
	unsubscribe()

	m.Broadcast(contestID, sse.Event{Type: sse.EventContestUpdated})

	select {
	case _, ok := <-ch:
		if ok {
			t.Fatal("received event after unsubscribe")
		}
	default:
		// Expected
	}
}
