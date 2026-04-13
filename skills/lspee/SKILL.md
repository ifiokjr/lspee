---
name: lspee
description: Guides agents through using lspee to access LSP servers for code intelligence — completions, definitions, references, diagnostics, and more.
---

# lspee agent skill

lspee gives you full Language Server Protocol access scoped to the project you're working in. Instead of grep and regex, use real code intelligence.

## Quick start

1. Check what LSPs are available for the project:
   ```bash
   lspee lsps --file src/main.rs --output json
   ```

2. Check or create project config:
   ```bash
   lspee config show --output json
   # or initialize if missing:
   lspee config init
   ```

3. Query what an LSP can do:
   ```bash
   lspee capabilities --lsp rust-analyzer --output json
   ```

4. Use `lspee do` for ergonomic LSP calls (recommended over raw `lspee call`):
   ```bash
   # Hover information
   lspee do hover --file src/main.rs --line 10 --col 5

   # Go to definition
   lspee do definition --file src/main.rs --line 10 --col 5

   # Find all references
   lspee do references --file src/main.rs --line 10 --col 5

   # Workspace symbol search
   lspee do workspace-symbols --lsp rust-analyzer --query "MyStruct"
   ```

5. For advanced use, send raw JSON-RPC requests:
   ```bash
   lspee call --lsp rust-analyzer --client-kind agent --output json --request '{"jsonrpc":"2.0","id":1,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///abs/path/src/main.rs"},"position":{"line":10,"character":5}}}'
   ```

## Working rules

- Always use `--output json` for structured output.
- Always pass `--client-kind agent` on `call` commands.
- Always use absolute file paths in URIs: `file:///absolute/path/to/file.rs`.
- The daemon auto-starts — you don't need to manage it.
- Session identity is `(project_root, lsp_id, config_hash)` — sessions are reused automatically.
- Check `lspee capabilities` before calling a method to confirm it's supported.
- Prefer `lspee do` over `lspee call` — it builds the JSON-RPC request for you and wraps responses with metadata.

## Common workflow

1. **Discover** → `lspee lsps --file <path> --output json`
2. **Capabilities** → `lspee capabilities --lsp <id> --output json`
3. **Call** → `lspee do hover --file <path> --line <n> --col <n>`
4. **Status** → `lspee status --output json`
5. **Configure** → `lspee config add-lsp --id <id> --command <cmd>`

## MCP server integration

lspee can run as an MCP (Model Context Protocol) server over stdio, exposing LSP tools for LLM integration:

```bash
lspee mcp [--project-root /path/to/project]
```

To configure in an MCP client (e.g. Claude Desktop), add to your MCP settings:

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

## All CLI commands

| Command                                             | Purpose                                                         |
| --------------------------------------------------- | --------------------------------------------------------------- |
| `lspee lsp`                                         | Show resolved config identity (project root, config hash, LSPs) |
| `lspee lsps`                                        | Discover available LSPs for a file                              |
| `lspee call`                                        | Send a raw JSON-RPC request to an LSP                           |
| `lspee do <method>`                                 | Ergonomic LSP dispatch (hover, definition, references, etc.)    |
| `lspee capabilities`                                | Query LSP server capabilities                                   |
| `lspee config show\|init\|add-lsp\|remove-lsp\|set` | Manage project configuration                                    |
| `lspee status`                                      | Daemon health check                                             |
| `lspee stop`                                        | Stop the daemon                                                 |
| `lspee restart`                                     | Restart the daemon                                              |
| `lspee serve`                                       | Run the daemon in the foreground                                |
| `lspee mcp`                                         | Start MCP server over stdio                                     |
| `lspee doctor`                                      | Environment and integration health checks                       |

## Guidance

For detailed command reference, JSON-RPC request formats, and configuration options, see [REFERENCE.md](./REFERENCE.md).
