export type ParticipantRole = "test_taker" | "examiner";
export type SessionStatus =
  | "waiting_for_examiner"
  | "ready"
  | "in_progress"
  | "completed"
  | "terminated"
  | "expired";
export type ConnectionStatus =
  "connecting" | "connected" | "reconnecting" | "disconnected";

export interface CardFront {
  readonly cardId: string;
  readonly position: number;
  readonly total: number;
  readonly front: string;
}

export interface CardFull extends CardFront {
  readonly back: string;
}

export interface TestTakerSnapshot {
  readonly role: "test_taker";
  readonly sessionId: string;
  readonly revision: number;
  readonly status: SessionStatus;
  readonly examinerConnected: boolean;
  readonly currentCard?: CardFront;
  readonly expiresAt: string;
}

export interface ExaminerSnapshot {
  readonly role: "examiner";
  readonly sessionId: string;
  readonly revision: number;
  readonly status: SessionStatus;
  readonly testTakerConnected: boolean;
  readonly currentCard?: CardFull;
  readonly expiresAt: string;
}

export type RoleSnapshot = TestTakerSnapshot | ExaminerSnapshot;

export type SessionUpdate =
  | {
      readonly kind: "participant_changed";
      readonly revision: number;
      readonly status: SessionStatus;
      readonly examinerConnected: boolean;
      readonly testTakerConnected: boolean;
    }
  | {
      readonly kind: "started";
      readonly revision: number;
      readonly card: CardFront | CardFull;
    }
  | {
      readonly kind: "card_changed";
      readonly revision: number;
      readonly card: CardFront | CardFull;
    }
  | {
      readonly kind: "ended";
      readonly revision: number;
      readonly status: "completed" | "terminated" | "expired";
    }
  | {
      readonly kind: "error";
      readonly code: string;
      readonly currentRevision: number;
    };

export interface JoinRequest {
  readonly sessionId: string;
  readonly role: ParticipantRole;
  readonly afterRevision: number;
}

export interface SessionCommandRequest {
  readonly sessionId: string;
  readonly commandId: string;
}

export type CommandName = "session:start" | "session:advance" | "session:end";
export type Ack =
  { readonly ok: true } | { readonly ok: false; readonly code: string };

export interface ClientToServerEvents {
  "session:join": (
    request: JoinRequest,
    acknowledge: (result: Ack) => void,
  ) => void;
  "session:start": (
    request: SessionCommandRequest,
    acknowledge: (result: Ack) => void,
  ) => void;
  "session:advance": (
    request: SessionCommandRequest,
    acknowledge: (result: Ack) => void,
  ) => void;
  "session:end": (
    request: SessionCommandRequest,
    acknowledge: (result: Ack) => void,
  ) => void;
}

export interface ServerToClientEvents {
  "session:snapshot": (snapshot: RoleSnapshot) => void;
  "session:update": (update: SessionUpdate) => void;
  "session:error": (error: {
    readonly code: string;
    readonly terminal: boolean;
  }) => void;
}
