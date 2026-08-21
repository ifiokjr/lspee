# CLI Reference

## `monokit lsp`

Show effective identity.

```bash
monokit lsp [--project-root <path>] [--output human|json]
```

## `monokit status`

Query daemon stats including memory totals/budgets.

```bash
monokit status [--project-root <path>] [--no-start-daemon] [--output human|json]
```

## `monokit call`

Send one synchronous JSON-RPC request through daemon.

```bash
monokit call \
  --lsp <id> \
  [--root <path>] \
  --request '<json|@file>' \
  [--client-kind agent|human|ci] \
  [--no-start-daemon] \
  [--output json|pretty]
```

## `monokit proxy`

Expose a daemon-backed LSP session over stdio for editors such as Helix.

```bash
monokit proxy --lsp <id> [--root <path>] [--no-start-daemon]
```

## `monokit lsps`

List matching LSPs for file extension.

```bash
monokit lsps --file <path> [--output human|json]
```

## `monokit serve`

Run daemon in foreground.

```bash
monokit serve [--project-root <path>]
```

## `monokit stop`

Gracefully stop daemon via control protocol.

```bash
monokit stop [--project-root <path>]
```

## `monokit restart`

Best-effort stop then start daemon.

```bash
monokit restart [--project-root <path>]
```

## `monokit doctor`

Environment and daemon readiness checks.

```bash
monokit doctor [--project-root <path>] [--output human|json]
```
