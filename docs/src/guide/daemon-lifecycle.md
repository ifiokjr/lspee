# Daemon Lifecycle

## Commands

- `monokit serve` — run daemon in foreground.
- `monokit status` — query daemon stats; auto-start by default.
- `monokit proxy` — attach editor-facing dedicated stream.
- `monokit stop` — graceful shutdown via control protocol.
- `monokit restart` — best-effort stop, then start.

## Auto-start behavior

`status` and `call` auto-start daemon unless `--no-start-daemon` is passed.

## Idle session eviction

- Daemon keeps sessions while active leases exist.
- Once unleased, a session is evicted after `idle_ttl_secs` of idle time (default: 300 seconds / 5 minutes).
- Configurable in `monokit.toml` under `[session]`:

{{#include ../includes/session-idle-config.md}}

- Eviction attempts graceful LSP shutdown, then force-stop fallback.

## Daemon auto-shutdown

{{#include ../includes/daemon-auto-shutdown.md}}

## Session shutdown on daemon stop

When daemon receives `Shutdown`, it:

1. stops accepting new work,
2. shuts down all in-memory sessions,
3. removes socket file,
4. exits.

## Operational recommendations

- Use one daemon per project root.
- Prefer explicit `--project-root` in CI/agent scripts.
- Use `monokit stop` in test cleanup.
