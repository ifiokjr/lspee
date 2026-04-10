# Lspee Daemon & Session Lifecycle (LSP Multiplexing)

## Goal

Define how `lspee` multiplexes Language Server Protocol (LSP) sessions via a background daemon so repeated CLI invocations can reuse warm server processes safely and predictably.

Core requirements:

- CLI is a short-lived client.
- Background broker owns LSP child processes.
- Session identity is keyed by `(project_root, config_hash, lsp_id)`.
- Transport uses local sockets.
- Lifecycle supports reference counting and activity timestamps.
- Idle sessions are garbage-collected after 60 seconds.

---

## Components

### 1. `lspee` CLI (foreground client)

Responsibilities:

- Resolve effective config (defaults + user + project + runtime overrides).
- Compute `config_hash` from normalized effective config relevant to LSP startup/runtime behavior.
- Determine `project_root` and `lsp_id`.
- Connect to local daemon socket.
- Request an attach/lease to the session key.
- Proxy stdio/JSON-RPC traffic between editor/tool and daemon stream endpoint.
- Release lease on shutdown.

The CLI should never directly spawn or own persistent LSP server processes.

### 2. Daemon Broker (background process)

Responsibilities:

- Accept local socket connections from CLI clients.
- Maintain session registry keyed by:
  - `project_root` (canonical absolute path)
  - `config_hash` (stable digest)
  - `lsp_id` (e.g. `rust-analyzer`, `gopls`)
- Create missing sessions on demand.
- Multiplex N client leases onto one backend LSP process.
- Track both:
  - `ref_count` (active attached clients)
  - `last_activity_at` (monotonic timestamp)
- Run periodic GC tick (e.g. every 1s).
- Stop sessions idle for `>= 60s` with `ref_count == 0`.
- Exit daemon when no sessions exist and no clients are connected for optional global idle timeout (implementation-defined).

### 3. Session Worker (per unique key)

Responsibilities:

- Spawn LSP child process with resolved command/env/cwd.
- Own stdin/stdout/stderr pipes.
- Translate/multiplex client-side requests/notifications to backend.
- Preserve LSP initialize state for reuse (warm session).
- Maintain per-session counters/metrics.
- Support graceful stop (`shutdown` + `exit`) and forced kill fallback.

---

## Identity Model

A session key is:

```text
SessionKey = (project_root, config_hash, lsp_id)
```

### Canonicalization rules

- `project_root`: canonicalized (`realpath`), symlink-resolved, platform-normalized casing where applicable.
- `config_hash`: hash over canonical serialized effective config subset that impacts LSP behavior:
  - chosen LSP command + args
  - env overrides
  - initialization options
  - root markers / workspace mode
  - transport-relevant flags
- `lsp_id`: normalized lowercase identifier.

If any of these change, broker creates a distinct session.

---

## Transport Model

## Primary control socket

Local IPC endpoint used by CLI and daemon:

- Unix (implemented): `<project_root>/.lspee/daemon.sock`
- Windows named pipes: planned, not yet implemented

Control protocol (framed JSON messages) examples:

- `Attach { session_key, client_meta }`
- `AttachOk { lease_id, stream_endpoint }`
- `Release { lease_id }`
- `Ping` / `Pong`
- `Stats { ... }`

Normative schema/versioning/error codes are defined in `docs/specs/daemon-protocol.md` (Protocol v1).

### Data stream socket(s)

After `AttachOk`, CLI either:

1. Uses a dedicated per-lease stream endpoint (preferred for isolation), or
2. Uses multiplexed channel IDs over the control socket.

Preferred default: separate stream endpoint per lease for simpler backpressure and failure isolation.

---

## Session State Machine

```text
Absent
  -> Spawning
  -> Initializing
  -> Ready
  -> Draining (ref_count==0)
  -> Terminating
  -> Absent
```

### State details

- **Spawning**: child process launch in progress.
- **Initializing**: first attach performs LSP `initialize` handshake.
- **Ready**: accepts attached clients.
- **Draining**: no active clients, waiting for idle timeout.
- **Terminating**: graceful shutdown then force kill if deadline exceeded.

On new attach during **Draining**, transition back to **Ready** and cancel pending GC.

---

## Lease and Activity Semantics

Each successful attach creates a `lease_id`.

- `ref_count` increments on attach, decrements on explicit `Release` or disconnect.
- `last_activity_at` updates on:
  - inbound client message to session
  - outbound server message from session
  - attach/release transitions

Idle eligibility:

```text
ref_count == 0 && now - last_activity_at >= 60s
```

Then session enters `Terminating`.

### Why both ref_count and activity timestamp

- `ref_count` ensures no active client gets terminated.
- `last_activity_at` avoids killing immediately after detach and supports race-safe grace period.

---

## 60s Idle GC Algorithm

Daemon runs periodic sweep every second:

1. Iterate session registry.
2. Skip sessions where `ref_count > 0`.
3. Compute idle duration from monotonic clock.
4. If idle `>= 60s`, mark `Terminating` and begin graceful shutdown.
5. Remove session from registry only after process exit/cleanup.

Graceful termination sequence:

1. Send LSP `shutdown` request with timeout (e.g. 3s).
2. Send LSP `exit` notification.
3. Wait short grace window.
4. If still alive, force kill child process.

---

## Concurrency & Race Handling

### Singleflight spawn

For the same `SessionKey`, concurrent `Attach` calls must coalesce:

- first caller creates session in `Spawning`
- others wait on same future/promise
- all receive same session when ready

### Late release / disconnect

If client crashes:

- daemon detects broken socket
- auto-releases associated lease
- updates `ref_count` and `last_activity_at`

### Attach during termination

If session in `Terminating` receives new attach:

- preferred: reject with retriable error `SessionRestarting`
- daemon immediately respawns fresh session and reattach path continues

Implementation may choose either as long as client retry UX is deterministic.

---

## Failure Behavior

- If LSP child exits unexpectedly:
  - mark session failed
  - notify attached clients
  - drop leases
  - allow next attach to respawn new session
- If daemon unavailable:
  - CLI can spawn daemon-on-demand (optional) then retry once.
- If control socket handshake fails auth/version check:
  - hard fail with actionable error.

---

## Observability

Minimum structured fields for logs/metrics:

- `session_key` parts (`project_root`, `config_hash`, `lsp_id`)
- `lease_id`
- `ref_count`
- `state`
- `idle_ms`
- spawn time, init time, uptime
- termination reason (`idle_gc`, `client_disconnect`, `crash`, `shutdown`)

Useful counters:

- `sessions_spawned_total`
- `sessions_reused_total`
- `sessions_gc_idle_total`
- `session_crashes_total`
- `attach_requests_total`

---

## Minimal End-to-End Flow

1. Editor invokes `lspee lsp --id rust-analyzer`.
2. CLI computes `(project_root, config_hash, lsp_id)`.
3. CLI connects to daemon, sends `Attach`.
4. Daemon finds/creates session.
5. Daemon returns `lease_id` + stream endpoint.
6. CLI proxies LSP traffic.
7. CLI exits/disconnects -> daemon releases lease.
8. Session sits in `Draining`.
9. At 60s idle, daemon shuts session down.

---

## Suggested Implementation Boundaries

- `lspee_cli`:
  - key computation
  - daemon discovery/launch
  - attach/release plumbing
- `lspee_daemon`:
  - socket server
  - registry + GC loop
  - session orchestration
- `lspee_lsp`:
  - LSP child process adapter
  - protocol bridging/multiplex helpers

This keeps ownership clear: CLI is stateless, daemon is lifecycle authority.
