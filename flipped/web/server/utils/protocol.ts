import type {
  CardFront,
  CardFull,
  ExaminerSnapshot,
  ParticipantRole,
  SessionStatus,
  SessionUpdate,
  TestTakerSnapshot,
} from "#shared/session";

function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("invalid_protocol_message");
  return value as Record<string, unknown>;
}
function text(value: unknown): string {
  if (typeof value !== "string") throw new Error("invalid_protocol_message");
  return value;
}
function number(value: unknown): number {
  const parsed =
    typeof value === "number"
      ? value
      : typeof value === "string"
        ? Number(value)
        : Number.NaN;
  if (!Number.isSafeInteger(parsed) || parsed < 0)
    throw new Error("invalid_protocol_message");
  return parsed;
}
function boolean(value: unknown): boolean {
  if (typeof value !== "boolean") throw new Error("invalid_protocol_message");
  return value;
}
function optionalObject(value: unknown): Record<string, unknown> | undefined {
  return value == null ? undefined : object(value);
}
function timestamp(value: unknown): string {
  const raw = object(value);
  return new Date(
    number(raw.seconds) * 1000 + number(raw.nanos ?? 0) / 1_000_000,
  ).toISOString();
}
function status(value: unknown): SessionStatus {
  const values: Record<string, SessionStatus> = {
    WAITING_FOR_EXAMINER: "waiting_for_examiner",
    READY: "ready",
    IN_PROGRESS: "in_progress",
    COMPLETED: "completed",
    TERMINATED: "terminated",
    EXPIRED: "expired",
  };
  const result = values[text(value)];
  if (!result) throw new Error("invalid_protocol_status");
  return result;
}
function front(value: unknown): CardFront {
  const raw = object(value);
  return {
    cardId: text(raw.cardId),
    position: number(raw.position),
    total: number(raw.total),
    front: text(raw.front),
  };
}
function full(value: unknown): CardFull {
  const raw = object(value);
  return { ...front(raw), back: text(raw.back) };
}

export function parseSnapshot(
  value: unknown,
  role: "test_taker",
): TestTakerSnapshot;
export function parseSnapshot(
  value: unknown,
  role: "examiner",
): ExaminerSnapshot;
export function parseSnapshot(
  value: unknown,
  role: ParticipantRole,
): TestTakerSnapshot | ExaminerSnapshot {
  const raw = object(value);
  const common = {
    sessionId: text(raw.sessionId),
    revision: number(raw.revision),
    status: status(raw.status),
    expiresAt: timestamp(raw.expiresAt),
  };
  if (role === "test_taker") {
    const card = optionalObject(raw.currentCard);
    return {
      role,
      ...common,
      examinerConnected: boolean(raw.examinerConnected),
      ...(card ? { currentCard: front(card) } : {}),
    };
  }
  const card = optionalObject(raw.currentCard);
  return {
    role,
    ...common,
    testTakerConnected: boolean(raw.testTakerConnected),
    ...(card ? { currentCard: full(card) } : {}),
  };
}

export function parseSnapshotResponse(
  value: unknown,
  role: "test_taker",
): TestTakerSnapshot;
export function parseSnapshotResponse(
  value: unknown,
  role: "examiner",
): ExaminerSnapshot;
export function parseSnapshotResponse(
  value: unknown,
  role: ParticipantRole,
): TestTakerSnapshot | ExaminerSnapshot {
  const raw = object(value);
  if (raw.result === "error") throw new Error(parseApplicationError(raw.error));
  if (raw.result !== "success") throw new Error("invalid_protocol_result");
  return parseSnapshot(object(raw.success).snapshot, role as "test_taker") as
    TestTakerSnapshot | ExaminerSnapshot;
}

export function parseCommandResponse(value: unknown): ExaminerSnapshot {
  const raw = object(value);
  if (raw.result === "error") throw new Error(parseApplicationError(raw.error));
  if (raw.result !== "success") throw new Error("invalid_protocol_result");
  return parseSnapshot(object(raw.success).snapshot, "examiner");
}

export function parseCreateResponse(value: unknown): {
  sessionId: string;
  token: string;
  invitation: string;
  expiresAt: string;
  cardCount: number;
  initialSnapshot: TestTakerSnapshot;
} {
  const raw = object(value);
  if (raw.result === "error") throw new Error(text(object(raw.error).code));
  if (raw.result !== "success") throw new Error("invalid_protocol_result");
  const success = object(raw.success);
  return {
    sessionId: text(success.sessionId),
    token: text(success.testTakerAccessToken),
    invitation: text(success.examinerInvitation),
    expiresAt: timestamp(success.expiresAt),
    cardCount: number(success.cardCount),
    initialSnapshot: parseSnapshot(success.initialSnapshot, "test_taker"),
  };
}

function parseApplicationError(value: unknown): string {
  return text(object(value).code);
}

export function parseWatchResponse(
  value: unknown,
  role: ParticipantRole,
): { snapshot?: TestTakerSnapshot | ExaminerSnapshot; update?: SessionUpdate } {
  const raw = object(value);
  if (raw.result === "snapshot")
    return {
      snapshot: parseSnapshot(raw.snapshot, role as "test_taker") as
        TestTakerSnapshot | ExaminerSnapshot,
    };
  if (raw.result === "error") {
    const error = object(raw.error);
    return {
      update: {
        kind: "error",
        code: text(error.code),
        currentRevision: number(error.currentRevision),
      },
    };
  }
  if (raw.result !== "event") throw new Error("invalid_protocol_result");
  const event = object(raw.event);
  const revision = number(event.revision);
  const payload = text(event.payload);
  if (payload === "participantChanged") {
    const changed = object(event.participantChanged);
    return {
      update: {
        kind: "participant_changed",
        revision,
        status: status(changed.status),
        examinerConnected: boolean(changed.examinerConnected),
        testTakerConnected: boolean(changed.testTakerConnected),
      },
    };
  }
  if (payload === "started" || payload === "cardChanged") {
    const rawPayload = object(event[payload]);
    const card =
      role === "examiner"
        ? full(rawPayload.currentCard)
        : front(rawPayload.currentCard);
    return {
      update: {
        kind: payload === "started" ? "started" : "card_changed",
        revision,
        card,
      },
    };
  }
  if (payload === "ended") {
    const endStatus = status(object(event.ended).status);
    if (!["completed", "terminated", "expired"].includes(endStatus))
      throw new Error("invalid_terminal_status");
    return {
      update: {
        kind: "ended",
        revision,
        status: endStatus as "completed" | "terminated" | "expired",
      },
    };
  }
  throw new Error("invalid_protocol_event");
}
