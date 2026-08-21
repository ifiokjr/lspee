# Troubleshooting

## `E_SESSION_SPAWN_FAILED`

Likely causes:

- LSP command not installed/in PATH
- unknown `--lsp` id without config override

Actions:

1. run `monokit doctor`
2. check `monokit lsps --file <file> --output json`
3. set `[[lsp]]` override in `monokit.toml`

## `failed to connect to daemon socket`

- daemon not started and `--no-start-daemon` used
- stale socket from old process

Actions:

```bash
monokit restart
```

## Unexpected response type

This usually indicates protocol mismatch or invalid request payload shape.

Actions:

- verify `jsonrpc`, `id`, `method`, `params`
- ensure client and daemon versions are aligned

## Reset everything

```bash
monokit stop
rm -rf .monokit
monokit status
```
