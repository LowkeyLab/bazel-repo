import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it } from "vitest";
import { useExaminerSessionStore } from "../../app/stores/examiner";
import { useTestTakerSessionStore } from "../../app/stores/test-taker";
import type { ExaminerSnapshot, TestTakerSnapshot } from "../../shared/session";

const testTakerSnapshot = (revision = 1): TestTakerSnapshot => ({
  role: "test_taker",
  sessionId: "session",
  revision,
  status: "ready",
  examinerConnected: true,
  expiresAt: "2030-01-01T00:00:00.000Z",
});
const examinerSnapshot = (revision = 1): ExaminerSnapshot => ({
  role: "examiner",
  sessionId: "session",
  revision,
  status: "ready",
  testTakerConnected: true,
  expiresAt: "2030-01-01T00:00:00.000Z",
});

describe("role-specific session stores", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("replaces only with newer authoritative test-taker state and rejects card backs", () => {
    const store = useTestTakerSessionStore();
    store.applySnapshot(testTakerSnapshot(2));
    store.applySnapshot(testTakerSnapshot(1));
    store.applySessionEvent({
      kind: "card_changed",
      revision: 3,
      card: {
        cardId: "card",
        position: 1,
        total: 1,
        front: "front",
        back: "secret",
      },
    });
    expect(store.snapshot?.revision).toBe(2);
    expect(store.snapshot?.currentCard).toBeUndefined();
  });

  it("applies examiner projections and waits for authoritative command outcomes", () => {
    const store = useExaminerSessionStore();
    store.applySnapshot(examinerSnapshot());
    store.commandPending("command");
    expect(store.snapshot?.status).toBe("ready");
    store.applySessionEvent({
      kind: "started",
      revision: 2,
      card: {
        cardId: "card",
        position: 1,
        total: 1,
        front: "front",
        back: "back",
      },
    });
    expect(store.snapshot?.status).toBe("in_progress");
    expect(store.snapshot?.currentCard?.back).toBe("back");
    expect(store.pendingCommandId).toBeUndefined();
  });

  it("marks reconnects and terminal events without fabricating a card", () => {
    const store = useTestTakerSessionStore();
    store.applySnapshot(testTakerSnapshot());
    store.connectionLost();
    expect(store.connection).toBe("reconnecting");
    store.applySessionEvent({
      kind: "ended",
      revision: 2,
      status: "terminated",
    });
    expect(store.snapshot?.status).toBe("terminated");
    expect(store.snapshot?.currentCard).toBeUndefined();
  });
});
