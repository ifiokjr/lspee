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

4. Make LSP calls:
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

## Common workflow

1. **Discover** → `lspee lsps --file <path> --output json`
2. **Capabilities** → `lspee capabilities --lsp <id> --output json`
3. **Call** → `lspee call --lsp <id> --request <json> --output json`
4. **Status** → `lspee status --output json`
5. **Configure** → `lspee config add-lsp --id <id> --command <cmd>`

## Guidance

For detailed command reference, JSON-RPC request formats, and configuration options, see [REFERENCE.md](./REFERENCE.md).
