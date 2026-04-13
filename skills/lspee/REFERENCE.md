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

### `lspee lsp` — show config identity

```bash
lspee lsp [--project-root <path>] [--output human|json]
```

Prints the resolved project root, config hash, and configured LSP servers. Useful for debugging session identity.

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

### `lspee call` — send a raw LSP request

```bash
lspee call \
  --lsp rust-analyzer \
  --client-kind agent \
  --output json \
  --request '{"jsonrpc":"2.0","id":1,"method":"textDocument/hover","params":{...}}'
```

Flags:

| Flag                             | Description                                          |
| -------------------------------- | ---------------------------------------------------- |
| `--lsp <id>`                     | LSP server identifier (required)                     |
| `--root <path>`                  | Override project root                                |
| `--request <json>`               | Raw JSON-RPC payload or `@path/to/file.json`         |
| `--client-kind agent\|human\|ci` | Caller kind for eviction priority (default: `human`) |
| `--output json\|pretty`          | Output format (default: `pretty`)                    |
| `--no-start-daemon`              | Disable daemon auto-start                            |

The request must be a valid JSON-RPC 2.0 message. The response is the LSP server's JSON-RPC response.

### `lspee do <method>` — ergonomic LSP dispatch

Execute LSP methods with structured flags instead of raw JSON-RPC. The LSP server is auto-resolved from the file extension when `--lsp` is omitted. Responses are wrapped with metadata (`lsp_id`, `method`, `file`, `position`, `elapsed_ms`). Location results (definition, references, implementation, type-definition) include a `context_line` field with the source text at each location.

#### Shared flags

All `lspee do` methods accept these flags:

| Flag                    | Description                                                          |
| ----------------------- | -------------------------------------------------------------------- |
| `--lsp <id>`            | LSP server identifier (auto-resolved from file extension if omitted) |
| `--root <path>`         | Override project root                                                |
| `--output json\|pretty` | Output format (default: `json`)                                      |
| `--no-start-daemon`     | Disable daemon auto-start                                            |

#### Position-based methods

These methods require `--file`, `--line`, and `--col` (zero-based):

```bash
# Hover — type info and documentation
lspee do hover --file src/main.rs --line 10 --col 5

# Definition — jump to symbol definition
lspee do definition --file src/main.rs --line 10 --col 5

# Implementation — find trait/interface implementations
lspee do implementation --file src/main.rs --line 10 --col 5

# Type definition — jump to the type's definition
lspee do type-definition --file src/main.rs --line 10 --col 5

# Completion — get completion suggestions
lspee do completion --file src/main.rs --line 10 --col 5

# Signature help — function parameter info
lspee do signature-help --file src/main.rs --line 10 --col 5
```

#### References

```bash
lspee do references --file src/main.rs --line 10 --col 5 [--include-declaration]
```

Extra flag: `--include-declaration` includes the symbol's declaration in results (default: false).

#### Rename

```bash
lspee do rename --file src/main.rs --line 10 --col 5 --new-name better_name
```

Extra flag: `--new-name <name>` (required) the new name for the symbol.

#### Code action

```bash
lspee do code-action --file src/main.rs --line 10 --col 5 [--end-line 10 --end-col 20]
```

Extra flags: `--end-line` and `--end-col` define the end of the selection range (defaults to the start position for a point selection).

#### File-only methods

These methods require only `--file`:

```bash
# Document symbols — list functions, classes, variables
lspee do symbols --file src/main.rs

# Diagnostics — pull-model diagnostics (LSP 3.17+)
lspee do diagnostics --file src/main.rs
```

#### Formatting

```bash
lspee do formatting --file src/main.rs [--tab-size 4] [--insert-spaces true]
```

Extra flags: `--tab-size <n>` (default: 4), `--insert-spaces` (default: true).

#### Workspace symbols

Requires `--lsp` (no file to auto-resolve from):

```bash
lspee do workspace-symbols --lsp rust-analyzer --query "MyStruct"
```

Extra flag: `--query <string>` (required) the search query. Use empty string for all symbols.

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
lspee stop [--project-root <path>]
lspee restart [--project-root <path>]
```

### `lspee doctor` — environment health check

```bash
lspee doctor [--project-root <path>] [--output human|json]
```

Runs health checks for environment and integration readiness: config resolution, daemon reachability, LSP binary availability, and more.

### `lspee serve` — run daemon in foreground

```bash
lspee serve [--project-root <path>] [--log-format human|json] [--log-file <path>]
```

### `lspee mcp` — MCP server over stdio

Start an MCP (Model Context Protocol) server over stdio, exposing lspee tools for LLM integration.

```bash
lspee mcp [--project-root /path/to/project]
```

#### MCP client configuration

To use with Claude Desktop or other MCP clients, add to your MCP settings JSON:

```json
{
	"mcpServers": {
		"lspee": {
			"command": "lspee",
			"args": ["mcp", "--project-root", "/path/to/project"]
		}
	}
}
```

## Logging and diagnostics

The daemon supports structured logging via the `tracing` framework for agent debugging.

### Environment variables

| Variable           | Default           | Description                                                    |
| ------------------ | ----------------- | -------------------------------------------------------------- |
| `LSPEE_LOG`        | `lspee=info,warn` | Log level filter (uses `tracing_subscriber::EnvFilter` syntax) |
| `LSPEE_LOG_FORMAT` | `human`           | Log format: `human` for readable, `json` for structured output |
| `LSPEE_LOG_FILE`   | stderr            | Write logs to a file instead of stderr                         |

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

| Code                       | Meaning                                  | Retryable |
| -------------------------- | ---------------------------------------- | --------- |
| `E_UNSUPPORTED_VERSION`    | Protocol version mismatch                | No        |
| `E_BAD_MESSAGE`            | Invalid JSON or envelope                 | No        |
| `E_INVALID_SESSION_KEY`    | Bad project_root, lsp_id, or config_hash | No        |
| `E_SESSION_SPAWN_FAILED`   | LSP process failed to start              | Yes       |
| `E_LEASE_NOT_FOUND`        | Lease expired or invalid                 | No        |
| `E_SESSION_EVICTED_MEMORY` | Session killed for memory pressure       | Yes       |
| `E_TIMEOUT`                | Request timed out                        | Yes       |
| `E_INTERNAL`               | Internal daemon error                    | Yes       |

## Important notes

- URIs must be absolute: `file:///absolute/path`. Relative paths will fail.
- Line and character positions are 0-indexed (LSP convention).
- The daemon auto-starts on first command and idles for 5 minutes by default.
- Multiple agents can share one LSP session when `(root, lsp_id, config_hash)` matches.
- For ephemeral jobs, run `lspee stop` when done to free resources.
- The built-in language catalog has 100+ LSP definitions as fallback when no project config exists.
