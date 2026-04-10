# Troubleshooting

## `E_SESSION_SPAWN_FAILED`

Likely causes:

- LSP command not installed/in PATH
- unknown `--lsp` id without config override

Actions:

1. run `lspee doctor`
2. check `lspee lsps --file <file> --output json`
3. set `[lsp]` override in `lspee.toml`

## `failed to connect to daemon socket`

- daemon not started and `--no-start-daemon` used
- stale socket from old process

Actions:

```bash
lspee restart
```

## Unexpected response type

This usually indicates protocol mismatch or invalid request payload shape.

Actions:

- verify `jsonrpc`, `id`, `method`, `params`
- ensure client and daemon versions are aligned

## Reset everything

```bash
lspee stop
rm -rf .lspee
lspee status
```
