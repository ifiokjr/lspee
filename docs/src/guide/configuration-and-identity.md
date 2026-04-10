# Configuration and Identity

## Config layers

Lowest to highest precedence:

1. built-in defaults
2. user config: `~/.config/lspee/config.toml`
3. project config: `<project_root>/lspee.toml`

## Session identity

`lspee` computes:

```text
(project_root, lsp_id, config_hash)
```

`config_hash` changes when LSP-relevant config changes.

## Why this matters

- Same identity = reused warm session
- Different identity = isolated session

This enables safe multiplexing for many agents in one codebase.

## Runtime command resolution

For `lspee call --lsp <id>` daemon resolves command/args in this order:

1. explicit project/user config for the same `lsp.id`
2. Helix-inspired default catalog (`defaults/languages.toml`)
3. error if still unresolved

## Recommended project config

```toml
workspace_mode = "single"
root_markers = [".git", "Cargo.toml"]

[lsp]
id = "rust-analyzer"
command = "rust-analyzer"
args = []
```
