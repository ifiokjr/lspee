# Language Catalog (Top 100 Defaults)

`lspee` ships with a Helix-inspired default LSP catalog at:

- `crates/lspee_config/defaults/languages.toml`

Current seed size: **100 LSP definitions**.

## What each entry contains

- command
- args
- file extensions
- root markers

## Query by file

```bash
lspee lsps --file src/main.rs --output json
```

## Override behavior

You can override command/args through layered config (`config.toml` / `lspee.toml`) via `[lsp]`.

Example:

```toml
[lsp]
id = "rust-analyzer"
command = "/opt/bin/rust-analyzer"
args = []
```

## Practical tip

If your environment has multiple servers for one language, keep the project config explicit to avoid ambiguity in automation.
