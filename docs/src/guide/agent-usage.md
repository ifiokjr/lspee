# Using monokit from Agents

`monokit` is optimized for machine orchestration.

## Use JSON output everywhere

{{#include ../includes/agent-cli-examples.md}}

## Deterministic root selection

Always pass explicit root:

```bash
monokit call --root /abs/project --lsp rust-analyzer --client-kind agent --request @request.json --output json
```

## Recommended request pattern

- Use one request per call.
- Use stable JSON-RPC ids.
- Handle non-zero exits and parse stderr for daemon error codes.

## Concurrency

Multiple agents can share one session when the session key {{#include ../includes/session-key-inline.md}} matches.

## Cleanup

For ephemeral jobs:

```bash
monokit stop --project-root /abs/project
```
