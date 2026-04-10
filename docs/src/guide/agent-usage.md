# Using lspee from Agents

`lspee` is optimized for machine orchestration.

## Use JSON output everywhere

```bash
lspee status --output json
lspee lsp --output json
lspee lsps --file src/main.rs --output json
lspee call --lsp rust-analyzer --request @request.json --output json
```

## Deterministic root selection

Always pass explicit root:

```bash
lspee call --root /abs/project --lsp rust-analyzer --client-kind agent --request @request.json --output json
```

## Recommended request pattern

- Use one request per call.
- Use stable JSON-RPC ids.
- Handle non-zero exits and parse stderr for daemon error codes.

## Concurrency

Multiple agents can share one session when `(root, lsp_id, config_hash)` matches.

## Cleanup

For ephemeral jobs:

```bash
lspee stop --project-root /abs/project
```
