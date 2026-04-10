# Configuration

`lspee` resolves configuration in layered order and computes deterministic session identity.

## Layering Order

The effective configuration is merged in this precedence order (lowest to highest):

1. Built-in defaults
2. User config: `~/.config/lspee/config.toml`
3. Project config: `<project_root>/lspee.toml`

Higher layers overwrite lower layers at field granularity.

## Project Root Resolution

By default, project root is the current working directory, canonicalized via realpath.

CLI supports a root override:

```bash
lspee lsp --project-root /path/to/repo
```

This override participates in identity and config file lookup.

## Identity Model

The daemon session identity uses:

```text
(project_root, config_hash, lsp_id)
```

`config_hash` is SHA-256 over:

1. Canonicalized `project_root`
2. Canonical TOML serialization of merged effective config

Any change in root path or merged config produces a distinct hash.

## Config Shape

Current top-level keys:

- `lsp.id`
- `lsp.command`
- `lsp.args`
- `lsp.env`
- `lsp.initialization_options`
- `root_markers`
- `workspace_mode`
- `transport_flags`

## Example

```toml
[lsp]
id = "rust-analyzer"
command = "rust-analyzer"
args = []

[lsp.env]
RUST_LOG = "error"

workspace_mode = "single"
root_markers = [".git", "Cargo.toml"]
```
