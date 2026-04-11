# Configuration Reference

## File precedence

{{#include ../includes/config-layers.md}}

## Supported keys

```toml
workspace_mode = "single"
root_markers = [".git", "Cargo.toml"]

[[lsp]]
id = "rust-analyzer"
command = "rust-analyzer"
args = []

[lsp.env]
RUST_LOG = "error"

[lsp.initialization_options]
# server-specific options
```

### Session configuration

{{#include ../includes/session-idle-config.md}}

### Memory configuration

{{#include ../includes/memory-config.md}}

## Identity hash

Hash input currently includes canonical project root and the merged effective config.

## Language catalog

Built-in catalog file:

- {{#include ../includes/catalog-path.md}}

The catalog powers:

- `lspee lsps --file ...`
- runtime fallback command resolution for `lspee call --lsp <id>`

## Notes

`[session]` controls session lifecycle policy. `idle_ttl_secs` sets how long an unleased session stays alive. The default of 300 seconds (5 minutes) covers most agent workflows. Set higher for workflows with longer gaps between requests.

`[memory]` controls daemon policy, not LSP protocol behavior. It exists to protect local machine resources when many shared sessions are active.
