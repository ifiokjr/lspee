# lspee_daemon

Runtime/daemon crate for `lspee`.

## Responsibility

`lspee_daemon` owns long-running process behavior:

- daemon/session lifecycle,
- startup and shutdown flow,
- coordination between configuration and LSP transport components.

## What belongs here

- daemon state machine and orchestration,
- async runtime task coordination,
- integration of `lspee_config` + `lspee_lsp` at runtime.

## What must NOT belong here

- direct CLI parsing concerns,
- raw config schema ownership,
- protocol type definitions that should live in LSP-focused modules.

## Allowed internal dependencies

- `lspee_config`
- `lspee_lsp`

## Notes

This crate should act as the operational core. Keep policy decisions here, while keeping protocol details encapsulated in `lspee_lsp`.
