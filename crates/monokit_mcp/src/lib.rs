#![cfg_attr(not(unix), allow(unused))]

#[cfg(not(unix))]
compile_error!("monokit_mcp currently supports unix-like platforms only (linux/macOS)");

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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SummariseParams {
	/// Path to a directory to summarise. Recursively scans source files.
	pub path: String,
	/// Detail level: "index", "signature", or "outline".
	///   - index: file paths + public symbol names only (most compact)
	///   - signature: names, full type signatures, first doc line
	///   - outline: everything in signature + full doc comments, trait bounds
	#[serde(default = "default_level")]
	pub level: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SummariseFileParams {
	/// Path to a single source file to summarise.
	pub file: String,
	/// Detail level: "index", "signature", or "outline".
	#[serde(default = "default_level")]
	pub level: String,
}

fn default_level() -> String {
	"index".to_string()
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// MCP server that exposes monokit LSP capabilities as tools.
#[derive(Debug)]
pub struct MonokitMcpServer {
	/// Optional project root override supplied at startup.
	default_root: Option<PathBuf>,
}

impl MonokitMcpServer {
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
impl MonokitMcpServer {
	/// Discover available LSP servers for a given file extension.
	#[tool(
		name = "monokit_lsps",
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
		name = "monokit_capabilities",
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
		name = "monokit_call",
		description = "Send a raw JSON-RPC request to an LSP server through the monokit daemon"
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
		name = "monokit_status",
		description = "Get daemon status including session/lease counts, uptime, and memory usage"
	)]
	async fn status(&self, Parameters(params): Parameters<StatusParams>) -> String {
		let root = self.effective_root(params.root.as_deref());
		match daemon_helpers::query_status(root.as_deref()).await {
			Ok(json) => json,
			Err(err) => error_json(&err.to_string()),
		}
	}

	/// Show the resolved (merged) monokit configuration.
	#[tool(
		name = "monokit_config_show",
		description = "Show the resolved monokit configuration for the project"
	)]
	async fn config_show(&self, Parameters(params): Parameters<ConfigShowParams>) -> String {
		let root = self.effective_root(params.root.as_deref());
		match run_config_show(root.as_deref()) {
			Ok(json) => json,
			Err(err) => error_json(&err.to_string()),
		}
	}

	/// Summarise a directory of source files at three detail levels.
	#[tool(
		name = "monokit_summarise",
		description = "Summarise a codebase directory: list public symbols (index), signatures \
		               (signature), or full outline (outline)"
	)]
	async fn summarise(&self, Parameters(params): Parameters<SummariseParams>) -> String {
		let level = parse_level(&params.level);
		match run_summarise(&params.path, level) {
			Ok(text) => text,
			Err(err) => error_json(&err.to_string()),
		}
	}

	/// Summarise a single source file.
	#[tool(
		name = "monokit_summarise_file",
		description = "Summarise a single source file: list public symbols (index), signatures \
		               (signature), or full outline (outline)"
	)]
	async fn summarise_file(&self, Parameters(params): Parameters<SummariseFileParams>) -> String {
		let level = parse_level(&params.level);
		match run_summarise_file(&params.file, level) {
			Ok(text) => text,
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
		.map(|home| home.join(".config/monokit/config.toml"));

	let project_cfg = file_path
		.parent()
		.map(|parent| parent.join("monokit.toml"))
		.filter(|path| path.exists());

	// Find matching LSPs
	let matches = monokit_config::languages::lsps_for_file(
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
	let resolved = monokit_config::resolve(root)?;

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

fn parse_level(s: &str) -> monokit_summary::Level {
	match s {
		"signature" => monokit_summary::Level::Signature,
		"outline" => monokit_summary::Level::Outline,
		_ => monokit_summary::Level::Index,
	}
}

fn run_summarise(dir: &str, level: monokit_summary::Level) -> anyhow::Result<String> {
	let summary = monokit_summary::summarise(dir, level)?;
	Ok(monokit_summary::render_text(&summary))
}

fn run_summarise_file(file: &str, level: monokit_summary::Level) -> anyhow::Result<String> {
	let path = Path::new(file);
	let source = std::fs::read_to_string(path)
		.map_err(|e| anyhow::anyhow!("failed to read file '{file}': {e}"))?;
	let file_summary = monokit_summary::summarise_file(path, &source, level)?;
	Ok(monokit_summary::render_file_text(&file_summary))
}
