# monokit_daemon

[![codecov](https://codecov.io/gh/ifiokjr/monokit/branch/main/graph/badge.svg)](https://codecov.io/gh/ifiokjr/monokit)

[![Book](https://img.shields.io/badge/book-ifiokjr.github.io%2Fmonokit-blue)](https://ifiokjr.github.io/monokit/)

Runtime/daemon crate for `monokit`.

## Responsibility

`monokit_daemon` owns long-running process behavior:

- daemon/session lifecycle,
- startup and shutdown flow,
- coordination between configuration and LSP transport components.

## What belongs here

- daemon state machine and orchestration,
- async runtime task coordination,
- integration of `monokit_config` + `monokit_lsp` at runtime.

## What must NOT belong here

- direct CLI parsing concerns,
- raw config schema ownership,
- protocol type definitions that should live in LSP-focused modules.

## Allowed internal dependencies

- `monokit_config`
- `monokit_lsp`

## Notes

This crate should act as the operational core. Keep policy decisions here, while keeping protocol details encapsulated in `monokit_lsp`.

**Website:** <https://ifiokjr.github.io/monokit/>
