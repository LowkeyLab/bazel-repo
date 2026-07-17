# AGENTS.md (Flipped)

This file extends the repository-root `AGENTS.md` for all code under `flipped/`.

## Architecture

- Read `flipped/ARCHITECTURE.md` before changing service boundaries, session projections, authorization, or observability.
- Rust owns APKG import, credentials, authorization, and authoritative in-memory examination state.
- Nuxt is the browser UI and Socket.IO/gRPC/OAuth gateway; it must not duplicate authoritative domain state.
- Keep examiner and test-taker projections separate. A test-taker payload must never contain a card back.

## Observability and logging

- Application, domain, importer, and transport code emit reviewed `ServiceEvent` values through `EventDispatcher`.
- Do not use `println!`, `eprintln!`, `dbg!`, or `tracing::{trace!, debug!, info!, warn!, error!, event!}` as an alternative operational logging path in event-producing code.
- Tracing spans, subscribers, layers, and trace-context propagation are allowed; they serve distributed tracing rather than replacing typed operational events.
- `EventListener` implementations are sink adapters. They may write structured output directly or invoke tracing/OpenTelemetry APIs when that is the selected sink, provided they preserve the typed envelope and do not create recursion or duplicate delivery.
- Keep event fields allowlisted. Never emit raw access tokens, invitation values, JWT IDs, session IDs, participant IDs, uploaded content, or arbitrary error/debug strings.
- This policy is enforced through architecture, code review, and tests—not repository-wide Clippy bans—so legitimate listener implementations are not prohibited.

## Testing

- Unit-test event construction, redaction, listener failure isolation, queue bounds, and shutdown behavior.
- Integration tests should verify the longest realistic role-separated flow and transport behavior without duplicating unit-test edge cases.
