# Quick Start

## 1) Confirm daemon status

```bash
monokit status
```

If daemon is not running, this command starts it automatically.

## 2) Inspect effective identity

```bash
monokit lsp --output json
```

This shows:

- canonical project root
- effective config hash
- chosen/default LSP id

## 3) Discover likely LSPs for a file

```bash
monokit lsps --file src/main.rs --output human
```

## 4) Send a JSON-RPC request

```bash
monokit call \
  --lsp rust-analyzer \
  --request '{"jsonrpc":"2.0","id":1,"method":"workspace/symbol","params":{"query":"main"}}' \
  --output pretty
```

For automation:

```bash
monokit call --lsp rust-analyzer --request @request.json --output json
```

## 5) Run editor proxy when integrating with Helix

```bash
monokit proxy --lsp rust-analyzer --root /abs/project
```

## 6) Stop daemon

```bash
monokit stop
```
