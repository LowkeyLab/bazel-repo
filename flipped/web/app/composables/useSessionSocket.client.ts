import { onBeforeUnmount, onMounted } from "vue";
import { io, type Socket } from "socket.io-client";
import { v7 as uuidv7 } from "uuid";
import { useExaminerSessionStore } from "~/stores/examiner";
import { useTestTakerSessionStore } from "~/stores/test-taker";
import type {
  ClientToServerEvents,
  CommandName,
  ParticipantRole,
  ServerToClientEvents,
} from "#shared/session";
import { emitClientEvent } from "~/utils/observability";

export function useSessionSocket(role: ParticipantRole, sessionId: string) {
  const testTaker =
    role === "test_taker" ? useTestTakerSessionStore() : undefined;
  const examiner = role === "examiner" ? useExaminerSessionStore() : undefined;
  let socket: Socket<ServerToClientEvents, ClientToServerEvents> | undefined;
  const commandTimers = new Set<number>();

  const revision = () =>
    testTaker?.snapshot?.revision ?? examiner?.snapshot?.revision ?? 0;

  function connect() {
    if (socket) return;
    socket = io({ autoConnect: false, withCredentials: true });
    socket.on("connect", () => {
      emitClientEvent({
        name: "client.connection_changed",
        outcome: "success",
        role,
      });
      socket?.emit(
        "session:join",
        { sessionId, role, afterRevision: revision() },
        (result) => {
          if (!result.ok) (testTaker ?? examiner)?.fail(result.code);
        },
      );
    });
    socket.on("connect_error", (error) => {
      const code = [
        "origin_configuration_invalid",
        "origin_missing",
        "origin_rejected",
        "socket_capacity_exhausted",
        "unauthenticated",
      ].includes(error.message)
        ? error.message
        : "connection_failed";
      (testTaker ?? examiner)?.connectionLost(false);
      (testTaker ?? examiner)?.fail(code);
      emitClientEvent({
        name: "client.connection_changed",
        outcome: "failure",
        role,
        errorCode: code,
      });
    });
    socket.on("disconnect", () => {
      (testTaker ?? examiner)?.connectionLost(true);
      emitClientEvent({
        name: "client.connection_changed",
        outcome: "failure",
        role,
        errorCode: "disconnected",
      });
    });
    socket.on("session:snapshot", (snapshot) => {
      if (snapshot.role === "test_taker" && testTaker)
        testTaker.applySnapshot(snapshot);
      if (snapshot.role === "examiner" && examiner)
        examiner.applySnapshot(snapshot);
    });
    socket.on("session:update", (update) => {
      (testTaker ?? examiner)?.applySessionEvent(update);
    });
    socket.on("session:error", (error) => {
      (testTaker ?? examiner)?.fail(error.code);
      if (error.terminal) socket?.disconnect();
    });
    socket.connect();
  }

  function command(name: CommandName) {
    if (!socket || role !== "examiner") return;
    const commandId = uuidv7();
    examiner?.commandPending(commandId);
    const timer = window.setTimeout(() => {
      commandTimers.delete(timer);
      examiner?.fail("command_timeout");
      socket?.disconnect();
      socket?.connect();
    }, 10_000);
    commandTimers.add(timer);
    socket.emit(name, { sessionId, commandId }, (result) => {
      window.clearTimeout(timer);
      commandTimers.delete(timer);
      examiner?.commandCompleted(commandId);
      if (!result.ok) examiner?.fail(result.code);
    });
  }

  onMounted(connect);
  onBeforeUnmount(() => {
    for (const timer of commandTimers) window.clearTimeout(timer);
    commandTimers.clear();
    socket?.removeAllListeners();
    socket?.disconnect();
    socket = undefined;
  });

  return {
    start: () => command("session:start"),
    advance: () => command("session:advance"),
    end: () => command("session:end"),
  };
}
