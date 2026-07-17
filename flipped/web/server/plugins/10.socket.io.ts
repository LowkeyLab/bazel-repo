import type { ClientReadableStream } from "@grpc/grpc-js";
import { Server as Engine } from "engine.io";
import { Server } from "socket.io";
import type {
  ClientToServerEvents,
  ParticipantRole,
  ServerToClientEvents,
} from "#shared/session";
import {
  authCookieName,
  readCookieHeader,
  secureCookie,
} from "../utils/cookies";
import { executeCommand, getSnapshot, watchSession } from "../utils/grpc";
import { observability } from "../utils/observability";
import { parseWatchResponse } from "../utils/protocol";

interface SocketData {
  token: string;
  role?: ParticipantRole;
  sessionId?: string;
  admissionKey?: string;
  watch?: ClientReadableStream<unknown>;
}

function errorCode(error: unknown): string {
  if (error instanceof Error && error.message.length <= 96)
    return error.message;
  return "gateway_failure";
}

function normalizedOrigin(value: unknown): string | undefined {
  if (typeof value !== "string" || value.length > 256) return undefined;
  try {
    return new URL(value).origin;
  } catch {
    return undefined;
  }
}

function positiveLimit(value: unknown, fallback: number): number {
  const parsed = typeof value === "number" ? value : Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : fallback;
}

export default defineNitroPlugin((nitroApp) => {
  const config = useRuntimeConfig();
  const maxGlobalSockets = positiveLimit(config.maxGlobalSockets, 4_096);
  const maxSocketsPerSession = positiveLimit(config.maxSocketsPerSession, 8);
  const maxPendingSocketJoins = positiveLimit(
    config.maxPendingSocketJoins,
    128,
  );
  const admittedBySession = new Map<string, number>();
  let pendingSocketJoins = 0;
  const engine = new Engine({ maxHttpBufferSize: 64 * 1024 });
  const io = new Server<
    ClientToServerEvents,
    ServerToClientEvents,
    Record<string, never>,
    SocketData
  >();
  io.bind(engine);

  io.use((socket, next) => {
    if (io.sockets.sockets.size >= maxGlobalSockets)
      return next(new Error("socket_capacity_exhausted"));
    const origin = normalizedOrigin(socket.handshake.headers.origin);
    const allowedOrigin = normalizedOrigin(config.appOrigin);
    if (!allowedOrigin) return next(new Error("origin_configuration_invalid"));
    if (!origin) {
      if (socket.handshake.headers.host !== new URL(allowedOrigin).host)
        return next(new Error("origin_missing"));
    } else if (origin !== allowedOrigin) {
      return next(new Error("origin_rejected"));
    }
    const token = readCookieHeader(
      socket.handshake.headers.cookie,
      authCookieName(secureCookie(config.cookieSecure)),
    );
    if (!token) return next(new Error("unauthenticated"));
    socket.data.token = token;
    next();
  });

  io.on("connection", (socket) => {
    observability().emit(
      "INFO",
      {},
      { name: "socket.connected", outcome: "success" },
    );

    socket.on("session:join", async (request, acknowledge) => {
      if (
        !request ||
        typeof request.sessionId !== "string" ||
        !["test_taker", "examiner"].includes(request.role) ||
        !Number.isSafeInteger(request.afterRevision) ||
        request.afterRevision < 0
      ) {
        acknowledge({ ok: false, code: "invalid_request" });
        return;
      }
      if (
        socket.data.sessionId &&
        (socket.data.sessionId !== request.sessionId ||
          socket.data.role !== request.role)
      ) {
        acknowledge({ ok: false, code: "role_forbidden" });
        return;
      }
      const admissionKey = `${request.sessionId}:${request.role}`;
      if (
        (!socket.data.admissionKey &&
          (admittedBySession.get(admissionKey) ?? 0) >= maxSocketsPerSession) ||
        pendingSocketJoins >= maxPendingSocketJoins
      ) {
        acknowledge({ ok: false, code: "socket_capacity_exhausted" });
        return;
      }
      pendingSocketJoins += 1;
      socket.data.watch?.cancel();
      try {
        const snapshot =
          request.role === "test_taker"
            ? await getSnapshot(
                request.sessionId,
                "test_taker",
                socket.data.token,
              )
            : await getSnapshot(
                request.sessionId,
                "examiner",
                socket.data.token,
              );
        socket.data.role = request.role;
        socket.data.sessionId = request.sessionId;
        if (!socket.data.admissionKey) {
          socket.data.admissionKey = admissionKey;
          admittedBySession.set(
            admissionKey,
            (admittedBySession.get(admissionKey) ?? 0) + 1,
          );
        }
        socket.join(`session:${request.sessionId}:${request.role}`);
        socket.emit("session:snapshot", snapshot);
        const watch = watchSession(
          request.sessionId,
          request.role,
          snapshot.revision,
          socket.data.token,
        );
        socket.data.watch = watch;
        watch.on("data", (value) => {
          try {
            const parsed = parseWatchResponse(value, request.role);
            if (parsed.snapshot)
              socket.emit("session:snapshot", parsed.snapshot);
            if (parsed.update) socket.emit("session:update", parsed.update);
          } catch (error) {
            socket.emit("session:error", {
              code: errorCode(error),
              terminal: true,
            });
            watch.cancel();
          }
        });
        watch.on("error", (error) => {
          if (error.code !== 1)
            socket.emit("session:error", {
              code: errorCode(error),
              terminal: error.code === 16 || error.code === 5,
            });
        });
        observability().emit(
          "INFO",
          { sessionId: request.sessionId },
          {
            name: "socket.authenticated",
            outcome: "success",
            role: request.role,
          },
        );
        observability().emit(
          "INFO",
          { sessionId: request.sessionId },
          {
            name: "grpc.stream_started",
            outcome: "success",
            role: request.role,
          },
        );
        acknowledge({ ok: true });
      } catch (error) {
        const code = errorCode(error);
        observability().emit(
          "WARN",
          { sessionId: request.sessionId },
          {
            name: "socket.rejected",
            outcome: "rejected",
            errorCode: code,
            role: request.role,
          },
        );
        acknowledge({ ok: false, code });
      } finally {
        pendingSocketJoins -= 1;
      }
    });

    const command =
      (name: "start" | "advance" | "end") =>
      async (
        request: { sessionId: string; commandId: string },
        acknowledge: (
          result: { ok: true } | { ok: false; code: string },
        ) => void,
      ) => {
        if (
          socket.data.role !== "examiner" ||
          !socket.data.sessionId ||
          request?.sessionId !== socket.data.sessionId ||
          typeof request.commandId !== "string"
        ) {
          acknowledge({ ok: false, code: "role_forbidden" });
          return;
        }
        const started = performance.now();
        try {
          const snapshot = await executeCommand(
            name,
            socket.data.sessionId,
            request.commandId,
            socket.data.token,
          );
          socket.emit("session:snapshot", snapshot);
          observability().emit(
            "INFO",
            { sessionId: socket.data.sessionId, commandId: request.commandId },
            {
              name: "grpc.request_completed",
              outcome: "success",
              durationMs: Math.round(performance.now() - started),
              role: "examiner",
            },
          );
          acknowledge({ ok: true });
        } catch (error) {
          const code = errorCode(error);
          observability().emit(
            "WARN",
            { sessionId: socket.data.sessionId, commandId: request.commandId },
            {
              name: "grpc.request_completed",
              outcome: "rejected",
              errorCode: code,
              durationMs: Math.round(performance.now() - started),
              role: "examiner",
            },
          );
          acknowledge({ ok: false, code });
        }
      };
    socket.on("session:start", command("start"));
    socket.on("session:advance", command("advance"));
    socket.on("session:end", command("end"));

    socket.on("disconnect", () => {
      socket.data.watch?.cancel();
      if (socket.data.admissionKey) {
        const remaining =
          (admittedBySession.get(socket.data.admissionKey) ?? 1) - 1;
        if (remaining > 0)
          admittedBySession.set(socket.data.admissionKey, remaining);
        else admittedBySession.delete(socket.data.admissionKey);
      }
      observability().emit(
        "INFO",
        {
          ...(socket.data.sessionId
            ? { sessionId: socket.data.sessionId }
            : {}),
        },
        {
          name: "socket.disconnected",
          outcome: "success",
          ...(socket.data.role ? { role: socket.data.role } : {}),
        },
      );
    });
  });

  nitroApp.router.use(
    "/socket.io/",
    defineEventHandler({
      handler(event) {
        engine.handleRequest(event.node.req, event.node.res);
        event._handled = true;
      },
      websocket: {
        open(peer) {
          // @ts-expect-error Nitro's Node request is intentionally private while WebSockets are experimental.
          engine.prepare(peer._internal.nodeReq);
          // @ts-expect-error Engine.IO's Nitro bridge requires the private Node request.
          engine.onWebSocket(
            peer._internal.nodeReq,
            peer._internal.nodeReq.socket,
            peer.websocket,
          );
        },
      },
    }),
  );
  nitroApp.hooks.hookOnce(
    "close",
    () => new Promise<void>((resolve) => io.close(() => resolve())),
  );
});
