# Language Catalog Configuration (`languages.toml`)

This document summarizes the currently implemented language catalog behavior.

For full user-facing docs, see the mdBook pages:

- `docs/src/guide/language-catalog.md`
- `docs/src/reference/configuration.md`

## Current implementation

- Built-in catalog source: `crates/lspee_config/defaults/languages.toml`
- Catalog shape: `[lsp."<id>"]` tables (Helix-inspired)
- Seed size: 100 entries
- Used by:
  - `lspee lsps --file ...`
  - daemon runtime fallback for `lspee call --lsp <id>`

## Override path (implemented)

Layered config can override one selected LSP entry via `[lsp]` in:

1. `~/.config/lspee/config.toml`
2. `<project_root>/lspee.toml`

Example:

```toml
[lsp]
id = "rust-analyzer"
command = "/opt/custom/bin/rust-analyzer"
args = []
```

## Future scope

Multi-entry user/project `languages.toml` overlays and conditional policies are not yet implemented.
