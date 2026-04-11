# Memory Budgets and Eviction

`lspee` can enforce memory budgets for daemon-managed sessions.

## Configuration

{{#include ../includes/memory-config.md}}

## Semantics

- `max_session_mb` — evict any single backend session above this limit
- `max_total_mb` — evict sessions until combined usage falls below this limit
- `check_interval_ms` — how often daemon samples RSS

## Sampling

Current implementation samples RSS via `ps -o rss= -p <pid>` on Unix-like platforms.

## Eviction policy

Current policy is **idle LRU with editor protection bias**:

1. non-editor sessions before editor sessions
2. unleased / lower-activity sessions before active ones
3. older last-used sessions before newer ones
4. larger memory users as tie-breaker

## Client-facing behavior

When a session is evicted for memory pressure:

- agent-style `call` requests receive a structured daemon error
- dedicated stream clients (`lspee proxy`) receive a terminal stream error
- proxy converts that into an LSP `window/showMessage` warning for editors

## Resume instructions

The default resume hint is:

- re-attach or retry the request
- for Helix, restart the language server if it does not reconnect automatically
