# Configuration Reference

## File precedence

1. built-in defaults
2. `~/.config/lspee/config.toml`
3. `<project_root>/lspee.toml`

## Supported keys

```toml
workspace_mode = "single"
root_markers = [".git", "Cargo.toml"]

[lsp]
id = "rust-analyzer"
command = "rust-analyzer"
args = []

[lsp.env]
RUST_LOG = "error"

[lsp.initialization_options]
# server-specific options

[session]
# How long an idle session stays alive before eviction (default: 300).
idle_ttl_secs = 300

[memory]
max_session_mb = 2048
max_total_mb = 8192
check_interval_ms = 1000
```

## Identity hash

Hash input currently includes canonical project root and the merged effective config.

## Language catalog

Built-in catalog file:

- `crates/lspee_config/defaults/languages.toml`

The catalog powers:

- `lspee lsps --file ...`
- runtime fallback command resolution for `lspee call --lsp <id>`

## Notes

`[session]` controls session lifecycle policy. `idle_ttl_secs` sets how long an unleased session stays alive. The default of 300 seconds (5 minutes) covers most agent workflows. Set higher for workflows with longer gaps between requests.

`[memory]` controls daemon policy, not LSP protocol behavior. It exists to protect local machine resources when many shared sessions are active.
