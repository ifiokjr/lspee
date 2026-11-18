---
"lspee_cli": patch
"lspee_mcp": patch
---

Upgrade `rmcp` to 3.11.0.

This clears the OAuth metadata SSRF advisory (`RUSTSEC-2026-0457`) and the unbounded Streamable HTTP response advisory (`RUSTSEC-2026-0462`). `rmcp` 2.2.0 was the newest 2.x release and does not include those fixes.

`uuid` stays on 1.26.1. Newer `uuid` releases require Rust 1.89, and this repo's toolchain is 1.88.0.
