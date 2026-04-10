# Using lspee from Terminals/Editors

## Human-friendly output

Default output is readable:

```bash
lspee status
lspee lsp
lspee lsps --file src/main.rs
lspee call --lsp rust-analyzer --request @request.json --output pretty
```

## Common workflows

### Run editor proxy manually

```bash
lspee proxy --lsp rust-analyzer --root /abs/project
```

### Check daemon and sessions

```bash
lspee status
```

### Restart daemon after environment changes

```bash
lspee restart
```

### Stop daemon when done

```bash
lspee stop
```

## Editor/tool integration

You can wrap `lspee call` in scripts for diagnostics/refactors where you want daemon-managed server reuse.
