# Daemon Lifecycle

## Commands

- `lspee serve` — run daemon in foreground.
- `lspee status` — query daemon stats; auto-start by default.
- `lspee proxy` — attach editor-facing dedicated stream.
- `lspee stop` — graceful shutdown via control protocol.
- `lspee restart` — best-effort stop, then start.

## Auto-start behavior

`status` and `call` auto-start daemon unless `--no-start-daemon` is passed.

## Idle session eviction

- Daemon keeps sessions while active leases exist.
- Once unleased, a session is evicted after `idle_ttl_secs` of idle time (default: 300 seconds / 5 minutes).
- Configurable in `lspee.toml` under `[session]`:

{{#include ../includes/session-idle-config.md}}

- Eviction attempts graceful LSP shutdown, then force-stop fallback.

## Session shutdown on daemon stop

When daemon receives `Shutdown`, it:

1. stops accepting new work,
2. shuts down all in-memory sessions,
3. removes socket file,
4. exits.

## Operational recommendations

- Use one daemon per project root.
- Prefer explicit `--project-root` in CI/agent scripts.
- Use `lspee stop` in test cleanup.
