---
lspee_daemon: patch
lspee_lsp: patch
lspee_protocol: patch
lspee_config: patch
---

# Test coverage updates

Expand test coverage with integration and unit tests:

- daemon control IPC flow tests (attach/call/release/stats/shutdown)
- session reuse and key isolation tests
- LSP frame and runtime roundtrip tests
- protocol serialization roundtrip tests
- default catalog size and lookup tests