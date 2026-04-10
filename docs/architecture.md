# lspee Architecture

## Purpose

`lspee` is a local LSP broker that multiplexes shared LSP runtimes across callers.

It keeps expensive language servers warm and lets humans/agents make synchronous request/response calls through a lightweight CLI.

## Core invariants

- CLI is short-lived; daemon owns long-lived state.
- Session identity is `(project_root, lsp_id, config_hash)`.
- Matching identities reuse the same session runtime.
- Idle sessions with no leases are evicted after 60s.

## Components

### `lspee_cli`

- resolves project/config identity
- auto-starts daemon when needed (`status`, `call`)
- performs `Attach` / `Call` / `Release` / `Stats` / `Shutdown`
- supports human + JSON outputs

### `lspee_daemon`

- serves NDJSON control protocol over local socket
- manages session registry and lease lifecycle
- spawns session runtimes via `lspee_lsp`
- evicts idle sessions and handles graceful shutdown

### `lspee_lsp`

- launches LSP subprocesses
- handles JSON-RPC framing (`Content-Length`)
- matches requests to responses by `id`

### `lspee_config`

- merges default + user + project config
- computes deterministic config hash
- exposes Helix-inspired top-100 LSP catalog

### `lspee_protocol`

- canonical control message structs/constants
- shared by CLI and daemon

## Control protocol

Transport: NDJSON over Unix socket at:

```text
<project_root>/.lspee/daemon.sock
```

Primary request types:

- `Attach` / `AttachOk`
- `Call` / `CallOk`
- `Release` / `ReleaseOk`
- `Stats` / `StatsOk`
- `Shutdown` / `ShutdownOk`
- `Error`

## Session lifecycle

```text
Absent -> Spawning -> Ready -> Draining -> Terminating -> Absent
```

- `Attach` increments lease/refcount.
- `Release` decrements lease/refcount.
- no leases + idle >= 60s => eviction path.

## Bootstrap policy

When daemon spawns a fresh session runtime it sends:

1. `initialize`
2. `initialized`

This ensures first user `Call` can immediately target regular LSP methods.

## Platform scope

Current implementation targets Unix-like platforms (Linux/macOS).
