---
"lspee_daemon": patch
---

Keep the memory-eviction stream error available to dedicated clients that connect after the notice is published.

The daemon used to publish that error on a broadcast channel. A proxy that had not accepted the dedicated socket yet never received it, and `memory_budget_eviction_emits_stream_error` could block until CI timed out.
