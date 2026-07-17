import { createHmac, randomUUID } from "node:crypto";
import { metrics, trace } from "@opentelemetry/api";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-http";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-http";
import { resourceFromAttributes } from "@opentelemetry/resources";
import {
  MeterProvider,
  PeriodicExportingMetricReader,
} from "@opentelemetry/sdk-metrics";
import { BatchSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { NodeTracerProvider } from "@opentelemetry/sdk-trace-node";
import type {
  EventEnvelope,
  EventSeverity,
  ServiceEvent,
} from "#shared/observability";

export interface EventContext {
  readonly requestId?: string;
  readonly commandId?: string;
  readonly causationId?: string;
  readonly sessionId?: string;
}
export interface EventListener {
  onEvent(event: EventEnvelope): void;
}

class StructuredJsonListener implements EventListener {
  onEvent(event: EventEnvelope): void {
    process.stdout.write(`${JSON.stringify(event)}\n`);
  }
}

class OpenTelemetryListener implements EventListener {
  private readonly counter = metrics
    .getMeter("flipped-web")
    .createCounter("flipped_web_events");
  onEvent(envelope: EventEnvelope): void {
    this.counter.add(1, {
      event_name: envelope.eventName,
      outcome: envelope.event.outcome,
    });
    trace.getActiveSpan()?.addEvent(envelope.eventName, {
      "event.id": envelope.eventId,
      "event.outcome": envelope.event.outcome,
      ...(envelope.event.errorCode
        ? { "error.code": envelope.event.errorCode }
        : {}),
    });
  }
}

export class EventDispatcher {
  private sequence = 0;
  private dropped = 0;
  constructor(
    private readonly service: EventEnvelope["service"],
    private readonly hmacKey: Buffer,
    private readonly listeners: readonly EventListener[],
  ) {}
  emit(
    severity: EventSeverity,
    context: EventContext,
    event: ServiceEvent,
  ): void {
    const span = trace.getActiveSpan()?.spanContext();
    const envelope: EventEnvelope = {
      schemaVersion: 1,
      eventName: event.name,
      eventId: randomUUID(),
      sequence: ++this.sequence,
      occurredAt: new Date().toISOString(),
      severity,
      service: this.service,
      ...(span?.traceId ? { traceId: span.traceId, spanId: span.spanId } : {}),
      ...(context.requestId ? { requestId: context.requestId } : {}),
      ...(context.commandId ? { commandId: context.commandId } : {}),
      ...(context.causationId ? { causationId: context.causationId } : {}),
      ...(context.sessionId
        ? {
            sessionRef: createHmac("sha256", this.hmacKey)
              .update(context.sessionId)
              .digest("base64url"),
          }
        : {}),
      event,
    };
    for (const listener of this.listeners) {
      try {
        listener.onEvent(envelope);
      } catch {
        this.dropped += 1;
      }
    }
  }
  droppedEvents(): number {
    return this.dropped;
  }
}

let dispatcher: EventDispatcher | undefined;
let tracerProvider: NodeTracerProvider | undefined;
let meterProvider: MeterProvider | undefined;

function hmacKey(value: string): Buffer {
  const key = Buffer.from(value, "base64url");
  if (key.length !== 32)
    throw new Error("NUXT_OBSERVABILITY_HMAC_KEY must decode to 32 bytes");
  return key;
}

export function initializeObservability(): EventDispatcher {
  if (dispatcher) return dispatcher;
  const config = useRuntimeConfig();
  if (config.otlpEndpoint) {
    const endpoint = String(config.otlpEndpoint).replace(/\/$/, "");
    const resource = resourceFromAttributes({
      "service.name": "flipped-web",
      "service.version": String(config.serviceVersion),
      "deployment.environment.name": String(config.environment),
      "service.instance.id": String(config.instanceId),
    });
    tracerProvider = new NodeTracerProvider({
      resource,
      spanProcessors: [
        new BatchSpanProcessor(
          new OTLPTraceExporter({ url: `${endpoint}/v1/traces` }),
        ),
      ],
    });
    tracerProvider.register();
    meterProvider = new MeterProvider({
      resource,
      readers: [
        new PeriodicExportingMetricReader({
          exporter: new OTLPMetricExporter({ url: `${endpoint}/v1/metrics` }),
        }),
      ],
    });
    metrics.setGlobalMeterProvider(meterProvider);
  }
  dispatcher = new EventDispatcher(
    {
      name: "flipped-web",
      version: String(config.serviceVersion),
      environment: String(config.environment),
      instanceId: String(config.instanceId),
    },
    hmacKey(String(config.observabilityHmacKey)),
    [new StructuredJsonListener(), new OpenTelemetryListener()],
  );
  return dispatcher;
}

export function observability(): EventDispatcher {
  return dispatcher ?? initializeObservability();
}

export async function shutdownObservability(): Promise<void> {
  await Promise.all([tracerProvider?.shutdown(), meterProvider?.shutdown()]);
  tracerProvider = undefined;
  meterProvider = undefined;
  dispatcher = undefined;
}
