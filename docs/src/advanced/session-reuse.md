# Session Reuse and Multiplexing

## Reuse key

Sessions are keyed by:

{{#include ../includes/session-key.md}}

## What is shared

When keys match, callers share:

- one LSP process
- one initialized runtime state

## What is isolated

New session is created when any key part differs:

- different root
- different lsp id
- different effective config hash

## Singleflight spawning

Concurrent attaches for the same key are coalesced so only one spawn/initialize path runs.

## Lease model

- Attach grants `lease_id`
- Call uses `lease_id`
- Release decrements active reference count
