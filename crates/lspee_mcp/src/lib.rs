#![cfg_attr(not(unix), allow(unused))]

#[cfg(not(unix))]
compile_error!("lspee_mcp currently supports unix-like platforms only (linux/macOS)");

mod daemon_helpers;

use std::path::Path;
use std::path::PathBuf;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::schemars;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Parameter types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LspsParams {
	/// Path to a source file used to look up matching LSP servers.
	pub file: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CapabilitiesParams {
	/// LSP server identifier (e.g. "rust-analyzer").
	pub lsp_id: String,
	/// Override project root. Uses the current directory when omitted.
	pub root: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CallParams {
	/// LSP server identifier (e.g. "rust-analyzer").
	pub lsp_id: String,
	/// Raw JSON-RPC request to forward to the LSP server.
	pub request: String,
	/// Override project root. Uses the current directory when omitted.
	pub root: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatusParams {
	/// Override project root. Uses the current directory when omitted.
	pub root: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigShowParams {
	/// Override project root. Uses the current directory when omitted.
	pub root: Option<String>,
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// MCP server that exposes lspee LSP capabilities as tools.
#[derive(Debug)]
pub struct LspeeMcpServer {
	/// Optional project root override supplied at startup.
	default_root: Option<PathBuf>,
}

impl LspeeMcpServer {
	#[must_use]
	pub fn new(default_root: Option<PathBuf>) -> Self {
		Self { default_root }
	}

	/// Resolve the effective project root from an optional per-call override.
	fn effective_root(&self, per_call: Option<&str>) -> Option<PathBuf> {
		per_call
			.map(PathBuf::from)
			.or_else(|| self.default_root.clone())
	}
}

#[tool_router(server_handler)]
impl LspeeMcpServer {
	/// Discover available LSP servers for a given file extension.
	#[tool(
		name = "lspee_lsps",
		description = "Discover available LSP servers for a file path / extension"
	)]
	async fn lsps(&self, Parameters(params): Parameters<LspsParams>) -> String {
		match run_lsps(&params.file) {
			Ok(json) => json,
			Err(err) => error_json(&err.to_string()),
		}
	}

	/// Query the capabilities advertised by an LSP server.
	#[tool(
		name = "lspee_capabilities",
		description = "Query the capabilities (supported methods) of an LSP server"
	)]
	async fn capabilities(&self, Parameters(params): Parameters<CapabilitiesParams>) -> String {
		let root = self.effective_root(params.root.as_deref());
		match daemon_helpers::query_capabilities(&params.lsp_id, root.as_deref()).await {
			Ok(json) => json,
			Err(err) => error_json(&err.to_string()),
		}
	}

	/// Send a raw JSON-RPC request to an LSP server via the daemon.
	#[tool(
		name = "lspee_call",
		description = "Send a raw JSON-RPC request to an LSP server through the lspee daemon"
	)]
	async fn call(&self, Parameters(params): Parameters<CallParams>) -> String {
		let root = self.effective_root(params.root.as_deref());
		match daemon_helpers::raw_call(&params.lsp_id, &params.request, root.as_deref()).await {
			Ok(json) => json,
			Err(err) => error_json(&err.to_string()),
		}
	}

	/// Retrieve daemon status (sessions, leases, uptime, memory).
	#[tool(
		name = "lspee_status",
		description = "Get daemon status including session/lease counts, uptime, and memory usage"
	)]
	async fn status(&self, Parameters(params): Parameters<StatusParams>) -> String {
		let root = self.effective_root(params.root.as_deref());
		match daemon_helpers::query_status(root.as_deref()).await {
			Ok(json) => json,
			Err(err) => error_json(&err.to_string()),
		}
	}

	/// Show the resolved (merged) lspee configuration.
	#[tool(
		name = "lspee_config_show",
		description = "Show the resolved lspee configuration for the project"
	)]
	async fn config_show(&self, Parameters(params): Parameters<ConfigShowParams>) -> String {
		let root = self.effective_root(params.root.as_deref());
		match run_config_show(root.as_deref()) {
			Ok(json) => json,
			Err(err) => error_json(&err.to_string()),
		}
	}
}

// ---------------------------------------------------------------------------
// Tool implementations (non-daemon)
// ---------------------------------------------------------------------------

fn run_lsps(file: &str) -> anyhow::Result<String> {
	let file_path = Path::new(file);

	// Resolve config paths
	let user_cfg = std::env::var_os("HOME")
		.map(PathBuf::from)
		.map(|home| home.join(".config/lspee/config.toml"));

	let project_cfg = file_path
		.parent()
		.map(|parent| parent.join("lspee.toml"))
		.filter(|path| path.exists());

	// Find matching LSPs
	let matches = lspee_config::languages::lsps_for_file(
		file_path,
		user_cfg.as_deref(),
		project_cfg.as_deref().map(Path::new),
	)?;

	let payload = serde_json::json!({
		"file": file,
		"lsps": matches,
	});

	Ok(serde_json::to_string_pretty(&payload)?)
}

fn run_config_show(root: Option<&Path>) -> anyhow::Result<String> {
	let resolved = lspee_config::resolve(root)?;

	let payload = serde_json::json!({
		"project_root": resolved.project_root,
		"config_hash": resolved.config_hash,
		"config": resolved.merged,
	});

	Ok(serde_json::to_string_pretty(&payload)?)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn error_json(message: &str) -> String {
	serde_json::json!({ "error": message }).to_string()
}
