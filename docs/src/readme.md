# Introduction

`lspee` is a local LSP multiplexer.

It solves a common workflow problem: repeated short-lived tooling calls (from humans, agents, and subagents) repeatedly starting language servers for the same workspace.

Instead, `lspee` keeps warm language-server sessions behind a daemon and lets callers share them safely.

## Design goals

- Fast repeat requests in the same workspace.
- Deterministic session identity.
- Strong machine-readable interface for agents.
- Human-friendly defaults and output options.
- Safe resource management (configurable idle session eviction, memory budgets).

## Core model

A session is identified by:

```text
(project_root, lsp_id, config_hash)
```

When two callers use the same key, they reuse the same running LSP process.

## Who this is for

- Engineers running diagnostics/refactors from terminal scripts.
- AI coding agents and orchestration systems.
- Teams that want one local LSP broker per project.

Continue with [Installation](./getting-started/installation.md).
