---
"lspee_daemon": minor
"lspee_lsp": minor
"lspee_config": minor
---

Implement runtime LSP command resolution from the default language catalog and add daemon session bootstrap (`initialize` + `initialized`) when spawning shared sessions.

This makes `lspee call --lsp <id>` usable in fresh projects without requiring explicit `lspee.toml` command wiring for common servers.
