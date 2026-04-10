# Helix Integration

`lspee proxy` lets Helix share the same daemon-managed backend session used by agents.

## Why proxy mode exists

Helix expects a normal LSP process over stdio.

`lspee proxy` sits between Helix and the daemon:

- Helix speaks standard LSP to `lspee proxy`
- proxy attaches to daemon using a dedicated stream
- daemon forwards real LSP traffic to the shared backend runtime

## Command shape

```bash
lspee proxy --lsp rust-analyzer --root /abs/project
```

## Helix example

In `languages.toml` you can point Rust at `lspee`:

```toml
[[language]]
name = "rust"
language-servers = ["lspee-rust-analyzer"]

[language-server.lspee-rust-analyzer]
command = "lspee"
args = ["proxy", "--lsp", "rust-analyzer", "--root", "/abs/project"]
```

In practice you will usually wrap root resolution in a script so the current workspace path is passed correctly.

## Lifecycle behavior

`lspee proxy` intercepts editor-facing lifecycle messages:

- `initialize` → answered locally from daemon-cached backend initialize result
- `initialized` → swallowed locally
- `shutdown` → answered locally
- `exit` → proxy exits

Everything else is forwarded to the shared backend session.

## Memory pressure warnings

If daemon evicts the backend session for memory reasons, proxy sends an LSP `window/showMessage` warning to Helix before terminating.

That warning includes a resume hint, e.g. restart the language server or retry the action.
