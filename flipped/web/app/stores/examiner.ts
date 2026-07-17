import { defineStore } from "pinia";
import type {
  ConnectionStatus,
  ExaminerSnapshot,
  SessionStatus,
  SessionUpdate,
} from "#shared/session";

interface ExaminerStoreState {
  snapshot?: ExaminerSnapshot;
  connection: ConnectionStatus;
  pendingCommandId?: string;
  errorCode?: string;
}

export const useExaminerSessionStore = defineStore("examiner-session", {
  state: (): ExaminerStoreState => ({ connection: "connecting" }),
  getters: {
    status: (state): SessionStatus | undefined => state.snapshot?.status,
  },
  actions: {
    applySnapshot(snapshot: ExaminerSnapshot) {
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
          testTakerConnected: update.testTakerConnected,
        };
      } else if (update.kind === "started" || update.kind === "card_changed") {
        if (!("back" in update.card)) return;
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
