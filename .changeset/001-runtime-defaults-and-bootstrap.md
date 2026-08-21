---
"monokit_daemon": minor
"monokit_lsp": minor
"monokit_config": minor
---

Implement runtime LSP command resolution from the default language catalog and add daemon session bootstrap (`initialize` + `initialized`) when spawning shared sessions.

This makes `monokit call --lsp <id>` usable in fresh projects without requiring explicit `monokit.toml` command wiring for common servers.
