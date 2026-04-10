# First End-to-End Request

This walkthrough demonstrates a complete request path.

## Request file

Create `request.json`:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "workspace/symbol",
  "params": {
    "query": "main"
  }
}
```

## Run call

```bash
lspee call --lsp rust-analyzer --request @request.json --output json
```

## What happens internally

1. CLI resolves merged config (`defaults < user < project`).
2. CLI computes `(project_root, lsp_id, config_hash)`.
3. CLI sends `Attach` to daemon.
4. Daemon reuses/spawns matching session.
5. CLI sends `Call` with JSON-RPC payload.
6. Daemon forwards payload to LSP runtime and waits response.
7. CLI prints `CallOk.response`.
8. CLI sends `Release`.

## Expected output

- A JSON-RPC response object matching your request `id`.
- Non-zero exit with machine-readable daemon error if anything fails.
