# monokit

[![Book](https://img.shields.io/badge/book-ifiokjr.github.io%2Fmonokit-blue)](https://ifiokjr.github.io/monokit/) [![CI](https://github.com/ifiokjr/monokit/actions/workflows/ci.yml/badge.svg)](https://github.com/ifiokjr/monokit/actions/workflows/ci.yml) [![codecov](https://codecov.io/gh/ifiokjr/monokit/branch/main/graph/badge.svg)](https://codecov.io/gh/ifiokjr/monokit)

`monokit` is a local LSP multiplexer for fast, shared, per-workspace language-server access.

It is designed for both:

- **agents/automation** (deterministic JSON output and stable IPC), and
- **humans** (simple CLI flow and readable output).

## Platform support

Current release target: **Linux/macOS (Unix sockets)**.

Windows named-pipe support is not yet implemented.

## Workspace crates

| Crate                                         | Description                                           |
| --------------------------------------------- | ----------------------------------------------------- |
| [`monokit_cli`](crates/monokit_cli)           | `monokit` binary and command UX                       |
| [`monokit_daemon`](crates/monokit_daemon)     | Daemon socket server + session lifecycle              |
| [`monokit_lsp`](crates/monokit_lsp)           | JSON-RPC/LSP subprocess bridge                        |
| [`monokit_config`](crates/monokit_config)     | Config layering + identity hashing + language catalog |
| [`monokit_protocol`](crates/monokit_protocol) | Shared control-protocol models                        |
| [`monokit`](crates/monokit)                   | Reservation stub crate for package-name preservation  |

## Install / build

```bash
git clone https://github.com/ifiokjr/monokit.git
cd monokit
cargo build --release -p monokit_cli
cargo install --path crates/monokit_cli
```

## Quick usage

```bash
# Auto-start daemon if missing
monokit status

# Call an LSP through the daemon
monokit call --lsp rust-analyzer --request @request.json --output pretty

# Run an editor-facing proxy (e.g. for Helix)
monokit proxy --lsp rust-analyzer --root /abs/project

# Stop daemon
monokit stop
```

## Agent usage

```bash
monokit status --output json
monokit lsp --output json
monokit lsps --file src/main.rs --output json
monokit call --lsp rust-analyzer --request @request.json --output json
```

## Key behavior

- Session key: `(project_root, lsp_id, config_hash)`
- Idle eviction: configurable via `[session] idle_ttl_secs` (default: 300s / 5 minutes)
- Control transport: NDJSON over local Unix socket
- Dedicated proxy streams for editor integrations like Helix
- Runtime fallback: `--lsp <id>` resolves command/args from built-in top-100 LSP catalog when not explicitly configured
- Optional memory budgets for per-session and combined daemon-managed LSP memory

## Documentation

- **[Book](https://ifiokjr.github.io/monokit/)** — comprehensive guide with installation, usage, and reference
- **[API docs (docs.rs)](https://docs.rs/monokit)** — Rust API documentation

Build docs locally with mdBook:

```bash
cd docs
mdbook build
mdbook serve
```
