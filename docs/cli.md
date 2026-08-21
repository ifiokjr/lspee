# monokit CLI UX

`monokit` is designed for both humans and agents.

- Humans get readable defaults.
- Agents get deterministic JSON via `--output json`.
- Editors can use `monokit proxy` to share daemon-managed backend sessions.

## Primary commands

```bash
monokit serve
monokit status
monokit call --lsp rust-analyzer --request @request.json
monokit proxy --lsp rust-analyzer --root /abs/project
monokit lsps --file src/main.rs
monokit lsp
monokit stop
monokit restart
```

## Daemon lifecycle

- `monokit serve` runs daemon in foreground.
- `monokit status`, `monokit call`, and `monokit proxy` auto-start daemon by default when socket is missing.
- Disable auto-start with `--no-start-daemon`.

## Command details

### `monokit call`

Send one synchronous JSON-RPC request to a shared daemon-managed LSP session.

```bash
monokit call --lsp <id> [--root <path>] --request '<json|@file>' [--client-kind agent|human|ci] [--output json|pretty]
```

### `monokit proxy`

Expose a daemon-backed session over stdio for editors.

```bash
monokit proxy --lsp <id> [--root <path>] [--no-start-daemon]
```

### `monokit status`

Query daemon `Stats`, including current memory totals and configured memory budgets.

```bash
monokit status [--project-root <path>] [--output human|json]
```

### `monokit lsp`

Show effective identity from config resolution.

```bash
monokit lsp [--project-root <path>] [--output human|json]
```

### `monokit lsps`

List matching LSPs for a file extension.

```bash
monokit lsps --file <path> [--output human|json]
```

### `monokit stop`

Gracefully stop daemon.

### `monokit restart`

Restart daemon.

### `monokit doctor`

Environment checks for config, daemon socket, and common LSP binaries.
