# Daemon Internals

This chapter explains how the lspee daemon stays alive after the CLI
exits, how it detaches from the terminal, and how subsequent CLI
invocations reconnect to the same daemon. Understanding these mechanics
helps with debugging, deployment, and reasoning about resource lifetime.

## How the daemon is spawned

{{#include ../includes/daemon-spawn-mechanism.md}}

## Auto-start and reconnection

{{#include ../includes/daemon-auto-start.md}}

## The daemon event loop

{{#include ../includes/daemon-event-loop.md}}

## Process tree

{{#include ../includes/daemon-process-tree.md}}

## LSP child lifecycle

Each LSP backend (e.g. `rust-analyzer`, `gopls`) is spawned as a child
process of the daemon with `stdin` piped and `stdout` piped. The daemon
communicates with the LSP child using the standard Language Server
Protocol framing (`Content-Length` headers over stdio).

The session registry tracks each LSP child by its session key
(`project_root`, `lsp_id`, `config_hash`). When a CLI client sends an
`Attach` request with a matching key, the existing runtime is reused
rather than spawning a new process. A singleflight gate ensures that
concurrent attaches for the same key coalesce into a single spawn.

When a session has no active leases and exceeds the idle TTL
(default: 300 seconds), the eviction loop sends LSP `shutdown` + `exit`
to the child, waits up to 2 seconds, and force-kills if the child does
not exit. The session is then removed from the registry.

## Daemon auto-shutdown

{{#include ../includes/daemon-auto-shutdown.md}}

## Platform-specific behavior

{{#include ../includes/platform-persistence.md}}

## Source code pointers

The mechanisms described above live in these files:

| File | What it owns |
|------|-------------|
| `crates/lspee_cli/src/commands/client.rs` | `spawn_daemon()`, `connect()`, auto-start retry loop |
| `crates/lspee_daemon/src/lib.rs` | `Daemon::run()` event loop, control connection handler, session bootstrap |
| `crates/lspee_daemon/src/registry.rs` | `SessionRegistry`, `acquire_or_spawn()` singleflight, lease tracking |
| `crates/lspee_daemon/src/eviction.rs` | `EvictionLoop`, idle session cleanup, daemon auto-shutdown |
| `crates/lspee_daemon/src/memory.rs` | `MemoryMonitor`, RSS sampling, budget enforcement |
| `crates/lspee_daemon/src/stream.rs` | Dedicated stream endpoints for editor proxies |
| `crates/lspee_lsp/src/lib.rs` | `LspTransport::spawn()`, `LspRuntime`, LSP stdio framing |

## Integration tests

These behaviors are verified by integration tests in
[`crates/lspee_daemon/tests/control_ipc.rs`](https://github.com/ifiokjr/lspee/blob/main/crates/lspee_daemon/tests/control_ipc.rs):

| Test | What it verifies |
|------|-----------------|
| `idle_session_is_evicted_after_ttl` | Sessions with no active leases are evicted after `idle_ttl_secs` |
| `daemon_auto_shuts_down_when_idle` | Daemon exits on its own after `daemon_idle_ttl_secs` with zero sessions |
| `daemon_stays_alive_while_sessions_active` | Daemon does not auto-shutdown while sessions exist, even past the TTL |
| `same_session_key_reuses_spawned_worker` | Matching session keys reuse the same LSP process |
| `different_config_hashes_spawn_distinct_sessions` | Different config hashes create isolated sessions |
| `memory_budget_eviction_emits_stream_error` | Memory-budget eviction sends `StreamError` to dedicated stream clients |
