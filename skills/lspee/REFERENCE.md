# lspee reference

lspee is a local LSP multiplexer that gives agents fast, shared, per-workspace language-server access.

## Installation

```bash
# Via npm
npm install -g @ifi/lspee

# Via cargo
cargo install lspee_cli

# Skill package (for agent integration)
npm install -g @ifi/lspee-skill
```

## CLI commands

### `lspee lsps` — discover available LSPs

```bash
lspee lsps --file src/main.rs --output json
```

Returns which LSP servers match a file extension, whether the binary is installed, and what root markers apply.

### `lspee capabilities` — query LSP capabilities

```bash
lspee capabilities --lsp rust-analyzer --output json
```

Returns which LSP methods are supported (hover, definition, references, etc.) and server info. Always check this before calling an unfamiliar method.

### `lspee call` — send an LSP request

```bash
lspee call \
  --lsp rust-analyzer \
  --client-kind agent \
  --output json \
  --request '{"jsonrpc":"2.0","id":1,"method":"textDocument/hover","params":{...}}'
```

The request must be a valid JSON-RPC 2.0 message. The response is the LSP server's JSON-RPC response.

### `lspee config show` — view resolved config

```bash
lspee config show --output json
```

### `lspee config init` — create project config

```bash
lspee config init [--root /path/to/project]
```

Auto-detects LSPs from project markers (Cargo.toml, package.json, etc.).

### `lspee config add-lsp` — add an LSP to config

```bash
lspee config add-lsp --id taplo --command taplo --args "lsp stdio"
```

### `lspee config remove-lsp` — remove an LSP from config

```bash
lspee config remove-lsp --id taplo
```

### `lspee config set` — set a config value

```bash
lspee config set session.idle_ttl_secs 600
lspee config set memory.max_total_mb 4096
```

### `lspee status` — daemon health

```bash
lspee status --output json
```

### `lspee stop` / `lspee restart`

```bash
lspee stop
lspee restart
```

### `lspee doctor` — environment health check

```bash
lspee doctor --output json
```

### `lspee serve` — run daemon in foreground

```bash
lspee serve [--project-root <path>] [--log-format human|json] [--log-file <path>]
```

## Logging and diagnostics

The daemon supports structured logging for agent debugging.

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `LSPEE_LOG` | `lspee=info,warn` | Log level filter (uses `tracing_subscriber::EnvFilter` syntax) |
| `LSPEE_LOG_FORMAT` | `human` | Log format: `human` for readable, `json` for structured output |
| `LSPEE_LOG_FILE` | stderr | Write logs to a file instead of stderr |

### JSON log output

When running with `--log-format json` or `LSPEE_LOG_FORMAT=json`, the daemon emits structured JSON log lines:

```bash
LSPEE_LOG_FORMAT=json lspee serve --project-root /my/project
```

Agents can parse these for diagnostics — session lifecycle events, LSP spawn/shutdown, memory eviction, and errors are all captured with structured fields (`lsp_id`, `lease_id`, `rss_bytes`, etc.).

### Auto-started daemon logs

When the CLI auto-starts a daemon in the background, logs are written to `.lspee/daemon.log` in the project root. To view:

```bash
cat .lspee/daemon.log
# or with JSON format:
LSPEE_LOG_FORMAT=json lspee call --lsp rust-analyzer ...
cat .lspee/daemon.log | jq .
```

## JSON-RPC request format

All `lspee call` requests must be valid JSON-RPC 2.0. Common patterns:

### Hover

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "textDocument/hover",
  "params": {
    "textDocument": { "uri": "file:///absolute/path/to/file.rs" },
    "position": { "line": 10, "character": 5 }
  }
}
```

### Go to definition

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "textDocument/definition",
  "params": {
    "textDocument": { "uri": "file:///absolute/path/to/file.rs" },
    "position": { "line": 10, "character": 5 }
  }
}
```

### Find references

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "textDocument/references",
  "params": {
    "textDocument": { "uri": "file:///absolute/path/to/file.rs" },
    "position": { "line": 10, "character": 5 },
    "context": { "includeDeclaration": true }
  }
}
```

### Document symbols

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "textDocument/documentSymbol",
  "params": {
    "textDocument": { "uri": "file:///absolute/path/to/file.rs" }
  }
}
```

### Workspace symbols

```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "workspace/symbol",
  "params": { "query": "MyStruct" }
}
```

### Completions

```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "textDocument/completion",
  "params": {
    "textDocument": { "uri": "file:///absolute/path/to/file.rs" },
    "position": { "line": 10, "character": 5 }
  }
}
```

### Code actions

```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "textDocument/codeAction",
  "params": {
    "textDocument": { "uri": "file:///absolute/path/to/file.rs" },
    "range": {
      "start": { "line": 10, "character": 0 },
      "end": { "line": 10, "character": 20 }
    },
    "context": { "diagnostics": [] }
  }
}
```

### Rename

```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "method": "textDocument/rename",
  "params": {
    "textDocument": { "uri": "file:///absolute/path/to/file.rs" },
    "position": { "line": 10, "character": 5 },
    "newName": "better_name"
  }
}
```

### Formatting

```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "method": "textDocument/formatting",
  "params": {
    "textDocument": { "uri": "file:///absolute/path/to/file.rs" },
    "options": { "tabSize": 4, "insertSpaces": true }
  }
}
```

### Diagnostics (pull model)

```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "method": "textDocument/diagnostic",
  "params": {
    "textDocument": { "uri": "file:///absolute/path/to/file.rs" }
  }
}
```

## Configuration reference

Project config lives in `lspee.toml` at the project root.

```toml
# Multiple LSPs per project
[[lsp]]
id = "rust-analyzer"
command = "rust-analyzer"
args = []

[lsp.env]
RUST_LOG = "error"

[[lsp]]
id = "taplo"
command = "taplo"
args = ["lsp", "stdio"]

[session]
idle_ttl_secs = 300

[memory]
max_session_mb = 2048
max_total_mb = 8192
```

## Error codes

| Code | Meaning | Retryable |
|------|---------|-----------|
| `E_UNSUPPORTED_VERSION` | Protocol version mismatch | No |
| `E_BAD_MESSAGE` | Invalid JSON or envelope | No |
| `E_INVALID_SESSION_KEY` | Bad project_root, lsp_id, or config_hash | No |
| `E_SESSION_SPAWN_FAILED` | LSP process failed to start | Yes |
| `E_LEASE_NOT_FOUND` | Lease expired or invalid | No |
| `E_SESSION_EVICTED_MEMORY` | Session killed for memory pressure | Yes |
| `E_TIMEOUT` | Request timed out | Yes |
| `E_INTERNAL` | Internal daemon error | Yes |

## Important notes

- URIs must be absolute: `file:///absolute/path`. Relative paths will fail.
- Line and character positions are 0-indexed (LSP convention).
- The daemon auto-starts on first command and idles for 5 minutes by default.
- Multiple agents can share one LSP session when `(root, lsp_id, config_hash)` matches.
- For ephemeral jobs, run `lspee stop` when done to free resources.
- The built-in language catalog has 100+ LSP definitions as fallback when no project config exists.
