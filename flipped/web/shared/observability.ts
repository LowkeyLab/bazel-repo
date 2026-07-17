export type ServiceEventName =
  | "service.started"
  | "service.ready"
  | "service.stopping"
  | "service.stopped"
  | "upload.accepted"
  | "upload.completed"
  | "upload.failed"
  | "oauth.exchange_requested"
  | "oauth.exchange_succeeded"
  | "oauth.exchange_rejected"
  | "socket.connected"
  | "socket.authenticated"
  | "socket.rejected"
  | "socket.disconnected"
  | "socket.reconnected"
  | "grpc.request_completed"
  | "grpc.stream_started"
  | "grpc.stream_cancelled"
  | "projection.emitted"
  | "client.connection_changed";

export type EventOutcome = "success" | "rejected" | "cancelled" | "failure";
export type EventSeverity = "INFO" | "WARN" | "ERROR";

export interface ServiceEvent {
  readonly name: ServiceEventName;
  readonly outcome: EventOutcome;
  readonly errorCode?: string;
  readonly durationMs?: number;
  readonly role?: "test_taker" | "examiner";
}

export interface EventEnvelope {
  readonly schemaVersion: 1;
  readonly eventName: ServiceEventName;
  readonly eventId: string;
  readonly sequence: number;
  readonly occurredAt: string;
  readonly severity: EventSeverity;
  readonly service: {
    readonly name: string;
    readonly version: string;
    readonly environment: string;
    readonly instanceId: string;
  };
  readonly traceId?: string;
  readonly spanId?: string;
  readonly requestId?: string;
  readonly commandId?: string;
  readonly causationId?: string;
  readonly sessionRef?: string;
  readonly event: ServiceEvent;
}
