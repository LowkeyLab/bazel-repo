# AGENTS.md (Mindreadr Kotlin Backend)

Ktor-based backend for Mindreadr, built and run with Bazel. For repo-wide prerequisites and shared commands, see the root `AGENTS.md`.

## Paths & Targets

- Main class: `io.lowkeylab.mindreadr.app.ApplicationKt`
- Binary target: `//mindreadr/src/main/io/lowkeylab/mindreadr/app:Application`
- Resources: `mindreadr/src/main/resources` (`application.conf`, `logback.xml`, word lists)

## Build, Run, Watch

- Build everything under Mindreadr:

  ```bash
  bazel build //mindreadr/...
  ```

- Build the runnable binary:

  ```bash
  bazel build //mindreadr/src/main/io/lowkeylab/mindreadr/app:Application
  ```

- Run the server (Netty):

  ```bash
  bazel run //mindreadr/src/main/io/lowkeylab/mindreadr/app:Application
  ```

- Auto-reload during development (if you have `ibazel`):

  ```bash
  ibazel run //mindreadr/src/main/io/lowkeylab/mindreadr/app:Application
  ```

Note: `ibazel run //mindreadr` may fail depending on alias resolution. Prefer the explicit `:Application` target above.

## Configuration

Configuration is in `application.conf` and can be overridden with environment variables:

- `PORT`: server port (default `8080`)
- `FRONTEND_URL`: allowed host for CORS (default `localhost:4200`)

Fish shell example:

```fish
set -x PORT 8080
set -x FRONTEND_URL localhost:4200
ibazel run //mindreadr/src/main/io/lowkeylab/mindreadr/app:Application
```

## API Surface

- `GET /` → "Hello World!"
- `GET /health` → "OK"
- `GET /games` → list games
- `POST /games` → create game
- `GET /games/{id}` → game details
- `WS /games/{id}/live` → live game updates and input
  - Incoming messages: sealed type with `SubmitGuess { guess: string }`
  - Outgoing messages: `GameState`, `PlayerJoined`, `GameTerminated`, `Error`

Tip: Use any WebSocket client to connect (e.g., `wscat` or browser). Create a game via `POST /games`, then connect to `/games/{id}/live`.

## Testing

- Run all tests for this backend:

  ```bash
  bazel test //mindreadr/...
  ```

- Example: run game tests only:

  ```bash
  bazel test //mindreadr/src/test/io/lowkeylab/mindreadr/game:game
  ```

## Logs & Observability

- Logging: `logback.xml`
- Call ID and request logging are enabled; `X-Request-Id` is supported

## Dev Hygiene

- Format code and BUILD files before committing:

  ```bash
  bazel run format
  bazel run //tools:buildifier
  ```

## Troubleshooting

- Port already in use → change `PORT` or free the port
- CORS blocked → set `FRONTEND_URL` to your frontend origin (host:port)
- `ibazel run //mindreadr` fails → run the explicit target `//mindreadr/src/main/io/lowkeylab/mindreadr/app:Application`
