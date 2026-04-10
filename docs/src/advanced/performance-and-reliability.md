# Performance and Reliability

## Performance levers

- Keep requests in same root/id/config to maximize reuse.
- Prefer one daemon per project root.
- Avoid unnecessary config churn (changes hash, breaks reuse).

## Reliability levers

- Use `--output json` in automation and parse robustly.
- Always release leases (CLI does this automatically).
- Use `lspee restart` after environment/path changes.

## Observability

`lspee status --output json` exposes counters:

- sessions spawned/reused
- idle evictions
- attach request count

## Failure modes to watch

- missing LSP binaries
- malformed JSON-RPC payloads
- stale local environment/socket state
