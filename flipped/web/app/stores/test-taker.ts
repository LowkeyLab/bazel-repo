import { defineStore } from "pinia";
import type {
  ConnectionStatus,
  SessionStatus,
  SessionUpdate,
  TestTakerSnapshot,
} from "#shared/session";

interface TestTakerStoreState {
  snapshot?: TestTakerSnapshot;
  connection: ConnectionStatus;
  pendingCommandId?: string;
  errorCode?: string;
}

export const useTestTakerSessionStore = defineStore("test-taker-session", {
  state: (): TestTakerStoreState => ({ connection: "connecting" }),
  getters: {
    status: (state): SessionStatus | undefined => state.snapshot?.status,
  },
  actions: {
    applySnapshot(snapshot: TestTakerSnapshot) {
      if (snapshot.revision < (this.snapshot?.revision ?? 0)) return;
      this.snapshot = snapshot;
      this.connection = "connected";
      this.errorCode = undefined;
    },
    applySessionEvent(update: SessionUpdate) {
      if (update.kind === "error") {
        this.errorCode = update.code;
        return;
      }
      const snapshot = this.snapshot;
      if (!snapshot || update.revision <= snapshot.revision) return;
      if (update.kind === "participant_changed") {
        this.snapshot = {
          ...snapshot,
          revision: update.revision,
          status: update.status,
          examinerConnected: update.examinerConnected,
        };
      } else if (update.kind === "started" || update.kind === "card_changed") {
        if ("back" in update.card) return;
        this.snapshot = {
          ...snapshot,
          revision: update.revision,
          status: "in_progress",
          currentCard: update.card,
        };
      } else {
        this.snapshot = {
          ...snapshot,
          revision: update.revision,
          status: update.status,
          currentCard: undefined,
        };
      }
      this.pendingCommandId = undefined;
    },
    connectionLost(reconnecting = true) {
      this.connection = reconnecting ? "reconnecting" : "disconnected";
    },
    commandPending(commandId: string) {
      this.pendingCommandId = commandId;
    },
    commandCompleted(commandId: string) {
      if (this.pendingCommandId === commandId)
        this.pendingCommandId = undefined;
    },
    fail(code: string) {
      this.errorCode = code;
      this.pendingCommandId = undefined;
    },
    reset() {
      this.$reset();
    },
  },
});
