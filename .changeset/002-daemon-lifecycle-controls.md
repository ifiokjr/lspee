---
"lspee_cli": minor
"lspee_daemon": minor
"lspee_protocol": minor
---

Add daemon lifecycle control commands and protocol support:

- `Shutdown` / `ShutdownOk` control message types
- `lspee stop`
- `lspee restart`

Daemon now supports graceful shutdown through protocol rather than relying on out-of-band process killing.
