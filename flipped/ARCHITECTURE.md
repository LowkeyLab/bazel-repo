# Flipped Architecture

## Service boundaries

Flipped has three layers:

1. The browser communicates with Nuxt through HTTP and Socket.IO.
2. Nuxt acts as a gateway to the private Rust gRPC and OAuth endpoints.
3. Rust owns APKG import, credentials, authorization, and authoritative examination state.

The browser stores only role-specific projections. Examiner projections may include the current card back; test-taker projections never do.

## Typed observability pipeline

Operational events use one explicit path:

```text
Domain / importer / application / transport
                  |
            ServiceEvent
                  |
           EventDispatcher
          /       |        \
structured JSON  OTLP      tests/recording
  EventListener  listeners   listeners
```

Event-producing code constructs the reviewed `ServiceEvent` taxonomy and an allowlisted `EventContext`. `EventDispatcher` creates the common envelope, assigns sequence and time fields, isolates listener failures, and fan-outs the event to bounded listeners.

This path is intentionally distinct from Rust's tracing-span machinery. Tracing spans and W3C context propagation describe request causality. Typed service events describe operational outcomes. A tracing event macro is not a substitute for a `ServiceEvent`: without an installed subscriber it produces no output, and with the OpenTelemetry layer it may become only a span event rather than the required JSON record.

## Logging constraint

Domain, importer, application, and transport code must not bypass `EventDispatcher` with `println!`, `eprintln!`, `dbg!`, or direct tracing event macros. Bypassing the dispatcher would evade taxonomy review, redaction, listener isolation, metrics projection, and consistent JSON output.

`EventListener` implementations are the output boundary, not event producers. A listener may:

- serialize the typed envelope as newline-delimited JSON;
- project it into OpenTelemetry spans or metrics;
- adapt it to a tracing subscriber or another sink when required.

A listener-backed tracing adapter must retain the typed envelope, avoid feeding its own output back into `EventDispatcher`, and avoid duplicate delivery when another listener already exports the same signal.

The constraint is architectural rather than a global Clippy prohibition. Repository-wide macro bans cannot distinguish an improper producer call from a legitimate sink adapter. Reviewers should reject bypass paths and require tests for new listeners or event categories.

## Sensitive data

Events and metric labels must not contain raw session, participant, invitation, access-token, or JWT identifiers. Session correlation uses the configured HMAC-derived reference. Error fields contain stable allowlisted codes rather than arbitrary messages or stack traces.

## Failure behavior

Observability is diagnostic and must not alter authoritative examination behavior. Listener panics and failures are isolated; bounded queues drop rather than block domain work indefinitely; shutdown performs bounded flushing. External collector availability does not determine service readiness.
