//! `monokit` reserved crate.
//!
//! This crate intentionally provides a minimal API so the package name can be
//! reserved and referenced from documentation/release tooling.
//!
//! Use these crates for functionality:
//! - `monokit_cli` for the `monokit` binary
//! - `monokit_daemon` for daemon/session orchestration
//! - `monokit_lsp` for JSON-RPC/LSP process transport
//! - `monokit_config` for configuration loading/merging
//! - `monokit_protocol` for IPC wire models

/// Marker constant indicating this is a reservation crate.
pub const RESERVED_CRATE: &str = "monokit";
