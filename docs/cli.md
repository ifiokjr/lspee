# lspee CLI UX

`lspee` is designed for both humans and agents.

- Humans get readable defaults.
- Agents get deterministic JSON via `--output json`.
- Editors can use `lspee proxy` to share daemon-managed backend sessions.

## Primary commands

```bash
lspee serve
lspee status
lspee call --lsp rust-analyzer --request @request.json
lspee proxy --lsp rust-analyzer --root /abs/project
lspee lsps --file src/main.rs
lspee lsp
lspee stop
lspee restart
```

## Daemon lifecycle

- `lspee serve` runs daemon in foreground.
- `lspee status`, `lspee call`, and `lspee proxy` auto-start daemon by default when socket is missing.
- Disable auto-start with `--no-start-daemon`.

## Command details

### `lspee call`

Send one synchronous JSON-RPC request to a shared daemon-managed LSP session.

```bash
lspee call --lsp <id> [--root <path>] --request '<json|@file>' [--client-kind agent|human|ci] [--output json|pretty]
```

### `lspee proxy`

Expose a daemon-backed session over stdio for editors.

```bash
lspee proxy --lsp <id> [--root <path>] [--no-start-daemon]
```

### `lspee status`

Query daemon `Stats`, including current memory totals and configured memory budgets.

```bash
lspee status [--project-root <path>] [--output human|json]
```

### `lspee lsp`

Show effective identity from config resolution.

```bash
lspee lsp [--project-root <path>] [--output human|json]
```

### `lspee lsps`

List matching LSPs for a file extension.

```bash
lspee lsps --file <path> [--output human|json]
```

### `lspee stop`

Gracefully stop daemon.

### `lspee restart`

Restart daemon.

### `lspee doctor`

Environment checks for config, daemon socket, and common LSP binaries.
