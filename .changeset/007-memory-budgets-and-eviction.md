---
"lspee_daemon": minor
"lspee_config": minor
"lspee_protocol": minor
"lspee_cli": patch
---

Add daemon-side memory budgets and eviction policy for shared LSP sessions.

Highlights:

- new `[memory]` config section with per-session and total limits
- periodic RSS sampling for backend processes
- idle/LRU-biased eviction with editor protection bias
- structured memory eviction warnings and resume hints
- memory totals/limits included in `lspee status`
