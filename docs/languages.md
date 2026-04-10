# Language Registry

`lspee` ships with a built-in language-to-LSP registry inspired by Helix `languages.toml`.

The default catalog currently includes **100 pre-seeded LSP server entries** covering common languages and ecosystems.

Default registry file:

- `crates/lspee_config/defaults/languages.toml`

## Querying by File

Use the CLI to ask which LSP servers apply to a file:

```bash
lspee lsps --file src/main.rs
```

Output includes:

- matching LSP id
- command + arguments
- health (`ok` when executable is present in `PATH`, `missing` otherwise)
- root markers

## Override Hooks

Registry defaults are loaded first, then overlays are attempted from:

1. `~/.config/lspee/config.toml`
2. `<file-parent>/lspee.toml`

Current override hook uses `[lsp]` fields from layered config to replace command/args for a matching `lsp.id`.

Example:

```toml
[lsp]
id = "rust-analyzer"
command = "/opt/custom/bin/rust-analyzer"
args = []
```

This lets projects/users pin alternate server binaries while keeping built-in language mapping.
