# Platform Support

## Current support (v0.1.x)

- ✅ Linux
- ✅ macOS
- ❌ Windows (named pipe transport not implemented yet)

`lspee` currently uses Unix domain sockets (`tokio::net::UnixListener`/`UnixStream`) and emits a clear compile-time error on unsupported platforms.

## Platform-specific daemon behavior

{{#include ../includes/platform-persistence.md}}

## What is platform-independent

The core architecture is portable:

- **Session registry** — in-memory session tracking, lease management, singleflight spawning
- **NDJSON control protocol** — envelope format, request/response pairs, error codes
- **Eviction policy** — idle TTL, memory budgets, LRU with editor-protection bias
- **LSP child management** — `Content-Length` framing, `initialize`/`shutdown` lifecycle
- **Configuration** — TOML parsing, config layering, identity hashing

Only the transport layer (`UnixListener`/`UnixStream`) and the daemon spawning mechanism (`Stdio::null()` + orphan reparenting) are platform-specific, which limits the scope of any future port.
