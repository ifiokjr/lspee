---
"monokit_cli": minor
"monokit_daemon": minor
"monokit_protocol": minor
---

Add daemon lifecycle control commands and protocol support:

- `Shutdown` / `ShutdownOk` control message types
- `monokit stop`
- `monokit restart`

Daemon now supports graceful shutdown through protocol rather than relying on out-of-band process killing.
