# lspee

[![Book](https://img.shields.io/badge/book-ifiokjr.github.io%2Flspee-blue)](https://ifiokjr.github.io/lspee/)
[![CI](https://github.com/ifiokjr/lspee/actions/workflows/ci.yml/badge.svg)](https://github.com/ifiokjr/lspee/actions/workflows/ci.yml)

`lspee` is a local LSP multiplexer for fast, shared, per-workspace language-server access.

It is designed for both:

- **agents/automation** (deterministic JSON output and stable IPC), and
- **humans** (simple CLI flow and readable output).

## Platform support

Current release target: **Linux/macOS (Unix sockets)**.

Windows named-pipe support is not yet implemented.

## Workspace crates

| Crate | Description |
|-------|-------------|
| [`lspee_cli`](crates/lspee_cli) | `lspee` binary and command UX |
| [`lspee_daemon`](crates/lspee_daemon) | Daemon socket server + session lifecycle |
| [`lspee_lsp`](crates/lspee_lsp) | JSON-RPC/LSP subprocess bridge |
| [`lspee_config`](crates/lspee_config) | Config layering + identity hashing + language catalog |
| [`lspee_protocol`](crates/lspee_protocol) | Shared control-protocol models |
| [`lspee`](crates/lspee) | Reservation stub crate for package-name preservation |

## Install / build

```bash
git clone https://github.com/ifiokjr/lspee.git
cd lspee
cargo build --release -p lspee_cli
cargo install --path crates/lspee_cli
```

## Quick usage

```bash
# Auto-start daemon if missing
lspee status

# Call an LSP through the daemon
lspee call --lsp rust-analyzer --request @request.json --output pretty

# Run an editor-facing proxy (e.g. for Helix)
lspee proxy --lsp rust-analyzer --root /abs/project

# Stop daemon
lspee stop
```

## Agent usage

```bash
lspee status --output json
lspee lsp --output json
lspee lsps --file src/main.rs --output json
lspee call --lsp rust-analyzer --request @request.json --output json
```

## Key behavior

- Session key: `(project_root, lsp_id, config_hash)`
- Idle eviction: configurable via `[session] idle_ttl_secs` (default: 300s / 5 minutes)
- Control transport: NDJSON over local Unix socket
- Dedicated proxy streams for editor integrations like Helix
- Runtime fallback: `--lsp <id>` resolves command/args from built-in top-100 LSP catalog when not explicitly configured
- Optional memory budgets for per-session and combined daemon-managed LSP memory

## Documentation

- **[Book](https://ifiokjr.github.io/lspee/)** — comprehensive guide with installation, usage, and reference
- **[API docs (docs.rs)](https://docs.rs/lspee)** — Rust API documentation

Build docs locally with mdBook:

```bash
cd docs
mdbook build
mdbook serve
```
