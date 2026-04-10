# CLI Reference

## `lspee lsp`

Show effective identity.

```bash
lspee lsp [--project-root <path>] [--output human|json]
```

## `lspee status`

Query daemon stats including memory totals/budgets.

```bash
lspee status [--project-root <path>] [--no-start-daemon] [--output human|json]
```

## `lspee call`

Send one synchronous JSON-RPC request through daemon.

```bash
lspee call \
  --lsp <id> \
  [--root <path>] \
  --request '<json|@file>' \
  [--client-kind agent|human|ci] \
  [--no-start-daemon] \
  [--output json|pretty]
```

## `lspee proxy`

Expose a daemon-backed LSP session over stdio for editors such as Helix.

```bash
lspee proxy --lsp <id> [--root <path>] [--no-start-daemon]
```

## `lspee lsps`

List matching LSPs for file extension.

```bash
lspee lsps --file <path> [--output human|json]
```

## `lspee serve`

Run daemon in foreground.

```bash
lspee serve [--project-root <path>]
```

## `lspee stop`

Gracefully stop daemon via control protocol.

```bash
lspee stop [--project-root <path>]
```

## `lspee restart`

Best-effort stop then start daemon.

```bash
lspee restart [--project-root <path>]
```

## `lspee doctor`

Environment and daemon readiness checks.

```bash
lspee doctor [--project-root <path>] [--output human|json]
```
