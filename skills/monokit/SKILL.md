---
name: monokit
description: Guides agents through using monokit to access LSP servers for code intelligence — completions, definitions, references, diagnostics, and more.
---

# monokit agent skill

monokit gives you full Language Server Protocol access scoped to the project you're working in. Instead of grep and regex, use real code intelligence.

## Quick start

1. Check what LSPs are available for the project:
   ```bash
   monokit lsps --file src/main.rs --output json
   ```

2. Check or create project config:
   ```bash
   monokit config show --output json
   # or initialize if missing:
   monokit config init
   ```

3. Query what an LSP can do:
   ```bash
   monokit capabilities --lsp rust-analyzer --output json
   ```

4. Use `monokit do` for ergonomic LSP calls (recommended over raw `monokit call`):
   ```bash
   # Hover information
   monokit do hover --file src/main.rs --line 10 --col 5

   # Go to definition
   monokit do definition --file src/main.rs --line 10 --col 5

   # Find all references
   monokit do references --file src/main.rs --line 10 --col 5

   # Workspace symbol search
   monokit do workspace-symbols --lsp rust-analyzer --query "MyStruct"
   ```

5. For advanced use, send raw JSON-RPC requests:
   ```bash
   monokit call --lsp rust-analyzer --client-kind agent --output json --request '{"jsonrpc":"2.0","id":1,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///abs/path/src/main.rs"},"position":{"line":10,"character":5}}}'
   ```

## Working rules

- Always use `--output json` for structured output.
- Always pass `--client-kind agent` on `call` commands.
- Always use absolute file paths in URIs: `file:///absolute/path/to/file.rs`.
- The daemon auto-starts — you don't need to manage it.
- Session identity is `(project_root, lsp_id, config_hash)` — sessions are reused automatically.
- Check `monokit capabilities` before calling a method to confirm it's supported.
- Prefer `monokit do` over `monokit call` — it builds the JSON-RPC request for you and wraps responses with metadata.

## Common workflow

1. **Discover** → `monokit lsps --file <path> --output json`
2. **Capabilities** → `monokit capabilities --lsp <id> --output json`
3. **Call** → `monokit do hover --file <path> --line <n> --col <n>`
4. **Status** → `monokit status --output json`
5. **Configure** → `monokit config add-lsp --id <id> --command <cmd>`

## MCP server integration

monokit can run as an MCP (Model Context Protocol) server over stdio, exposing LSP tools for LLM integration:

```bash
monokit mcp [--project-root /path/to/project]
```

To configure in an MCP client (e.g. Claude Desktop), add to your MCP settings:

```json
{
	"mcpServers": {
		"monokit": {
			"command": "monokit",
			"args": ["mcp", "--project-root", "/path/to/project"]
		}
	}
}
```

## All CLI commands

| Command                                               | Purpose                                                         |
| ----------------------------------------------------- | --------------------------------------------------------------- |
| `monokit lsp`                                         | Show resolved config identity (project root, config hash, LSPs) |
| `monokit lsps`                                        | Discover available LSPs for a file                              |
| `monokit call`                                        | Send a raw JSON-RPC request to an LSP                           |
| `monokit do <method>`                                 | Ergonomic LSP dispatch (hover, definition, references, etc.)    |
| `monokit capabilities`                                | Query LSP server capabilities                                   |
| `monokit config show\|init\|add-lsp\|remove-lsp\|set` | Manage project configuration                                    |
| `monokit status`                                      | Daemon health check                                             |
| `monokit stop`                                        | Stop the daemon                                                 |
| `monokit restart`                                     | Restart the daemon                                              |
| `monokit serve`                                       | Run the daemon in the foreground                                |
| `monokit mcp`                                         | Start MCP server over stdio                                     |
| `monokit doctor`                                      | Environment and integration health checks                       |

## Guidance

For detailed command reference, JSON-RPC request formats, and configuration options, see [REFERENCE.md](./REFERENCE.md).
