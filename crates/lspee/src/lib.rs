//! `lspee` reserved crate.
//!
//! This crate intentionally provides a minimal API so the package name can be
//! reserved and referenced from documentation/release tooling.
//!
//! Use these crates for functionality:
//! - `lspee_cli` for the `lspee` binary
//! - `lspee_daemon` for daemon/session orchestration
//! - `lspee_lsp` for JSON-RPC/LSP process transport
//! - `lspee_config` for configuration loading/merging
//! - `lspee_protocol` for IPC wire models

/// Marker constant indicating this is a reservation crate.
pub const RESERVED_CRATE: &str = "lspee";
