# Changelog

All notable changes to this project are documented in this file.

## [0.1.0] - 2026-03-09

### Added

- Unix daemon control server with NDJSON IPC and shared protocol models.
- Attach/Call/Release/Stats/Shutdown control flow.
- Session registry with lease tracking and 60s idle eviction.
- JSON-RPC LSP subprocess bridge with Content-Length framing.
- Agent + human CLI output modes.
- Daemon lifecycle commands: `serve`, `stop`, `restart`.
- `lspee proxy` for editor-facing shared LSP sessions.
- Per-session and total memory budgets with eviction warnings.
- Helix-inspired default language catalog with 100 LSP definitions.
- mdBook documentation under `docs/src`.
- Release hardening assets (licenses, CI workflow, changesets).
- Stub crate `lspee` for package-name reservation.

### Changed

- Runtime command resolution now falls back to the default language catalog by `--lsp <id>`.
- Daemon session spawn includes deterministic bootstrap (`initialize` + `initialized`).

### Fixed

- Removed scaffold-only call path; `lspee call` now forwards real JSON-RPC payloads.
