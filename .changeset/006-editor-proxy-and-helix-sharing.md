---
"lspee_cli": minor
"lspee_daemon": minor
"lspee_protocol": minor
---

Add editor-facing proxy support so Helix and other stdio-based clients can share daemon-managed backend LSP sessions.

Highlights:

- new `lspee proxy` command
- dedicated per-lease stream sockets for editor traffic
- cached backend initialize result returned to editor-facing `initialize`
- proxy warnings surfaced to editors via `window/showMessage`
