# Using monokit from Terminals/Editors

## Human-friendly output

Default output is readable:

```bash
monokit status
monokit lsp
monokit lsps --file src/main.rs
monokit call --lsp rust-analyzer --request @request.json --output pretty
```

## Common workflows

### Run editor proxy manually

```bash
monokit proxy --lsp rust-analyzer --root /abs/project
```

### Check daemon and sessions

```bash
monokit status
```

### Restart daemon after environment changes

```bash
monokit restart
```

### Stop daemon when done

```bash
monokit stop
```

## Editor/tool integration

You can wrap `monokit call` in scripts for diagnostics/refactors where you want daemon-managed server reuse.
