//! Ergonomic LSP method wrappers for the `lspee do` command.
//!
//! Instead of constructing raw JSON-RPC requests, callers specify structured
//! arguments and receive enriched responses with metadata. The LSP server is
//! auto-resolved from the file extension when `--lsp` is omitted.
//!
//! # Supported methods
//!
//! **Position-based** (`--file --line --col`): `hover`, `definition`,
//! `references`, `implementation`, `type-definition`, `completion`,
//! `signature-help`
//!
//! **Position + extra**: `rename` (`--new-name`), `code-action`
//! (`--end-line --end-col`)
//!
//! **File-only** (`--file`): `formatting`, `symbols`, `diagnostics`
//!
//! **Query-only** (`--query`): `workspace-symbols` (requires `--lsp`)
//!
//! # Output format
//!
//! Responses are wrapped with metadata:
//!
//! ```json
//! {
//!   "lsp_id": "rust-analyzer",
//!   "method": "textDocument/hover",
//!   "file": "src/main.rs",
//!   "position": {"line": 10, "character": 5},
//!   "result": { "..." },
//!   "elapsed_ms": 42
//! }
//! ```
//!
//! For `definition`, `references`, `implementation`, and `type-definition`,
//! location results are enriched with a `context_line` field containing
//! the source text at that location.
//!
//! # Document sync
//!
//! When the request targets a file (has a `textDocument` parameter),
//! `textDocument/didOpen` is sent before the main request and
//! `textDocument/didClose` is sent after. This uses the daemon's `Notify`
//! protocol to avoid blocking on fire-and-forget LSP notifications.

use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::time::Instant;

use anyhow::Result;
use clap::Args;
use clap::Subcommand;
use clap::ValueEnum;
use lspee_config::languages;
use lspee_daemon::Attach;
use lspee_daemon::AttachCapabilities;
use lspee_daemon::Call;
use lspee_daemon::CallOk;
use lspee_daemon::ClientKind;
use lspee_daemon::ClientMeta;
use lspee_daemon::ControlEnvelope;
use lspee_daemon::Notify;
use lspee_daemon::Release;
use lspee_daemon::SessionKeyWire;
use lspee_daemon::StreamMode;
use lspee_daemon::TYPE_ATTACH;
use lspee_daemon::TYPE_ATTACH_OK;
use lspee_daemon::TYPE_CALL;
use lspee_daemon::TYPE_CALL_OK;
use lspee_daemon::TYPE_NOTIFY;
use lspee_daemon::TYPE_NOTIFY_OK;
use lspee_daemon::TYPE_RELEASE;
use lspee_daemon::TYPE_RELEASE_OK;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::Lines;
use url::Url;

use super::client;

// ---------------------------------------------------------------------------
// Output format
// ---------------------------------------------------------------------------

/// Controls the JSON output format for `lspee do` responses.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DoOutput {
	/// Compact single-line JSON (best for agents and automation).
	Json,
	/// Pretty-printed JSON with indentation (best for humans).
	Pretty,
}

// ---------------------------------------------------------------------------
// Shared arguments (flattened into each method's args)
// ---------------------------------------------------------------------------

/// Arguments shared by all `lspee do` methods.
///
/// These are flattened into every method subcommand so that flags like
/// `--lsp` and `--output` appear naturally after the method name:
///
/// ```text
/// lspee do hover --file src/main.rs --line 10 --col 5
/// lspee do hover --lsp rust-analyzer --file src/main.rs --line 10 --col 5
/// ```
#[derive(Debug, Clone, Args)]
pub struct SharedArgs {
	/// LSP server identifier to target (e.g. `rust-analyzer`, `pyright`).
	///
	/// When omitted the server is auto-resolved from the file extension
	/// using the built-in language catalog and any project/user overrides.
	/// Use `lspee lsps --file <path>` to see which servers match a file.
	#[arg(long = "lsp")]
	pub lsp: Option<String>,

	/// Override the project root used for config resolution and session
	/// identity. Defaults to the current working directory.
	#[arg(long)]
	pub root: Option<PathBuf>,

	/// Do not auto-start the daemon if the control socket is missing.
	/// By default the daemon is spawned automatically on first use.
	#[arg(long)]
	pub no_start_daemon: bool,

	/// Response output format. Defaults to compact JSON for agent use.
	#[arg(long, value_enum, default_value_t = DoOutput::Json)]
	pub output: DoOutput,
}

// ---------------------------------------------------------------------------
// Per-method argument structs
// ---------------------------------------------------------------------------

/// Arguments for position-based methods (hover, definition, etc.).
///
/// The position uses zero-based line and column numbers matching the LSP
/// protocol. Column offsets are measured in UTF-16 code units.
#[derive(Debug, Clone, Args)]
pub struct PositionArgs {
	/// Shared flags (--lsp, --root, --output, --no-start-daemon).
	#[command(flatten)]
	pub shared: SharedArgs,

	/// Path to the source file (absolute or relative to the current
	/// working directory). The file must exist so its URI can be resolved.
	#[arg(long)]
	pub file: PathBuf,

	/// Zero-based line number (matches LSP protocol convention).
	/// Line 0 is the first line of the file.
	#[arg(long)]
	pub line: u32,

	/// Zero-based column offset in UTF-16 code units (matches LSP
	/// protocol convention). Column 0 is the start of the line.
	#[arg(long)]
	pub col: u32,
}

/// Arguments for `references` — position plus include-declaration toggle.
#[derive(Debug, Clone, Args)]
pub struct ReferencesArgs {
	/// Position arguments (--file, --line, --col) and shared flags.
	#[command(flatten)]
	pub position: PositionArgs,

	/// Include the declaration of the symbol in the results.
	/// By default only references (not the declaration) are returned.
	#[arg(long, default_value_t = false)]
	pub include_declaration: bool,
}

/// Arguments for `rename` — position plus the new name.
#[derive(Debug, Clone, Args)]
pub struct RenameArgs {
	/// Position arguments (--file, --line, --col) and shared flags.
	#[command(flatten)]
	pub position: PositionArgs,

	/// The new name to give the symbol. Must be a valid identifier in the
	/// target language.
	#[arg(long)]
	pub new_name: String,
}

/// Arguments for `code-action` — position plus optional end range.
///
/// When `--end-line` and `--end-col` are omitted the range collapses to
/// the single position given by `--line` and `--col` (point selection).
#[derive(Debug, Clone, Args)]
pub struct CodeActionArgs {
	/// Position arguments (--file, --line, --col) and shared flags.
	#[command(flatten)]
	pub position: PositionArgs,

	/// Zero-based end line of the selection range. Defaults to `--line`.
	#[arg(long)]
	pub end_line: Option<u32>,

	/// Zero-based end column of the selection range. Defaults to `--col`.
	#[arg(long)]
	pub end_col: Option<u32>,
}

/// Arguments for file-only methods (symbols, diagnostics).
#[derive(Debug, Clone, Args)]
pub struct FileOnlyArgs {
	/// Shared flags (--lsp, --root, --output, --no-start-daemon).
	#[command(flatten)]
	pub shared: SharedArgs,

	/// Path to the source file (absolute or relative to the current
	/// working directory).
	#[arg(long)]
	pub file: PathBuf,
}

/// Arguments for the `formatting` method.
#[derive(Debug, Clone, Args)]
pub struct FormattingArgs {
	/// Shared flags (--lsp, --root, --output, --no-start-daemon).
	#[command(flatten)]
	pub shared: SharedArgs,

	/// Path to the source file to format (absolute or relative to the
	/// current working directory).
	#[arg(long)]
	pub file: PathBuf,

	/// Number of spaces per indentation level.
	#[arg(long, default_value_t = 4)]
	pub tab_size: u32,

	/// Use spaces for indentation instead of tabs.
	#[arg(long, default_value_t = true)]
	pub insert_spaces: bool,
}

/// Arguments for `workspace-symbols`. Requires `--lsp` because there is
/// no file to auto-resolve from.
#[derive(Debug, Clone, Args)]
pub struct WorkspaceSymbolArgs {
	/// LSP server identifier to target. Required for workspace-symbols
	/// because there is no `--file` to infer the server from.
	#[arg(long = "lsp")]
	pub lsp: String,

	/// Override the project root used for config resolution and session
	/// identity. Defaults to the current working directory.
	#[arg(long)]
	pub root: Option<PathBuf>,

	/// Do not auto-start the daemon if the control socket is missing.
	#[arg(long)]
	pub no_start_daemon: bool,

	/// Response output format. Defaults to compact JSON for agent use.
	#[arg(long, value_enum, default_value_t = DoOutput::Json)]
	pub output: DoOutput,

	/// The search query string. Use an empty string to list all symbols.
	#[arg(long)]
	pub query: String,
}

// ---------------------------------------------------------------------------
// DoMethod enum (subcommand dispatch)
// ---------------------------------------------------------------------------

/// The LSP method to execute.
///
/// Each variant maps to a specific LSP protocol method and carries the
/// arguments needed to build the JSON-RPC request.
#[derive(Debug, Clone, Subcommand)]
pub enum DoMethod {
	/// Query hover information (type info, documentation) at a file
	/// position.
	///
	/// Sends `textDocument/hover`. Returns type signatures, doc comments,
	/// and other information the LSP server provides for the symbol at
	/// the given position.
	Hover(PositionArgs),

	/// Jump to the definition of the symbol at a file position.
	///
	/// Sends `textDocument/definition`. Returns one or more locations
	/// where the symbol is defined. Results include a `context_line`
	/// field with the source text at each location.
	Definition(PositionArgs),

	/// Find all references to the symbol at a file position.
	///
	/// Sends `textDocument/references`. Returns every location where
	/// the symbol is used. Use `--include-declaration` to also include
	/// the symbol's declaration. Results include a `context_line` field.
	References(ReferencesArgs),

	/// Find implementations of a trait, interface, or abstract method
	/// at a file position.
	///
	/// Sends `textDocument/implementation`. Results include a
	/// `context_line` field with the source text at each location.
	Implementation(PositionArgs),

	/// Jump to the type definition of the symbol at a file position.
	///
	/// Sends `textDocument/typeDefinition`. Returns the location where
	/// the symbol's type is defined. Results include a `context_line`
	/// field.
	#[command(name = "type-definition")]
	TypeDefinition(PositionArgs),

	/// Request completion suggestions at a file position.
	///
	/// Sends `textDocument/completion`. Returns a list of completion
	/// items the LSP server suggests at the cursor position.
	Completion(PositionArgs),

	/// Request function signature help at a file position.
	///
	/// Sends `textDocument/signatureHelp`. Returns parameter information
	/// for the function call surrounding the cursor.
	#[command(name = "signature-help")]
	SignatureHelp(PositionArgs),

	/// Rename the symbol at a file position across the workspace.
	///
	/// Sends `textDocument/rename`. Returns a workspace edit describing
	/// all the changes needed to rename the symbol to `--new-name`.
	Rename(RenameArgs),

	/// Request available code actions (quick-fixes, refactors) for a
	/// selection range.
	///
	/// Sends `textDocument/codeAction`. When `--end-line` and `--end-col`
	/// are omitted the range is a single point at `--line`/`--col`.
	#[command(name = "code-action")]
	CodeAction(CodeActionArgs),

	/// Format an entire document.
	///
	/// Sends `textDocument/formatting`. Returns an array of text edits
	/// to apply. Use `--tab-size` and `--insert-spaces` to control
	/// indentation style (defaults: 4 spaces).
	Formatting(FormattingArgs),

	/// List all symbols in a document (functions, classes, variables, …).
	///
	/// Sends `textDocument/documentSymbol`. Returns a flat or
	/// hierarchical list of symbols defined in the file.
	Symbols(FileOnlyArgs),

	/// Search for symbols across the entire workspace by name.
	///
	/// Sends `workspace/symbol`. Requires `--lsp` because there is no
	/// file to auto-resolve the server from. Pass an empty `--query ""`
	/// to list all workspace symbols.
	#[command(name = "workspace-symbols")]
	WorkspaceSymbols(WorkspaceSymbolArgs),

	/// Request pull-model diagnostics for a file.
	///
	/// Sends `textDocument/diagnostic` (LSP 3.17+). Not all servers
	/// support this — some only push diagnostics via notifications.
	/// Check `lspee capabilities --lsp <id>` to verify support.
	Diagnostics(FileOnlyArgs),
}

// ---------------------------------------------------------------------------
// DoCommand (top-level Args wrapper)
// ---------------------------------------------------------------------------

/// Execute an LSP method with ergonomic flags — no raw JSON-RPC required.
///
/// Auto-resolves the LSP server from the file extension when `--lsp` is
/// omitted. Use `--lsp` to override or when no file is involved (e.g.
/// `workspace-symbols`).
///
/// Positions use zero-based line and column numbers matching the LSP
/// protocol convention.
#[derive(Debug, Clone, Args)]
pub struct DoCommand {
	/// The LSP method to execute.
	#[command(subcommand)]
	pub method: DoMethod,
}

// ---------------------------------------------------------------------------
// RequestMeta — captured at build time for response wrapping
// ---------------------------------------------------------------------------

/// Metadata captured when building the JSON-RPC request, used later to
/// wrap the response with context.
#[derive(Debug, Clone)]
pub struct RequestMeta {
	/// The LSP protocol method name (e.g. `textDocument/hover`).
	pub lsp_method: String,
	/// The original file path argument, if applicable.
	pub file_path: Option<PathBuf>,
	/// The position within the file, if applicable.
	pub position: Option<LspPosition>,
}

/// A zero-based line/character position matching the LSP `Position` type.
#[derive(Debug, Clone, Serialize)]
pub struct LspPosition {
	/// Zero-based line number.
	pub line: u32,
	/// Zero-based character offset (UTF-16 code units).
	pub character: u32,
}

// ---------------------------------------------------------------------------
// DoMethod helpers
// ---------------------------------------------------------------------------

impl DoMethod {
	/// Extract the [`SharedArgs`] from whichever variant is active.
	///
	/// For `WorkspaceSymbols` a synthetic `SharedArgs` is built from its
	/// dedicated fields.
	pub fn shared_args(&self) -> SharedArgs {
		match self {
			Self::Hover(a)
			| Self::Definition(a)
			| Self::Completion(a)
			| Self::Implementation(a)
			| Self::TypeDefinition(a)
			| Self::SignatureHelp(a) => a.shared.clone(),
			Self::References(a) => a.position.shared.clone(),
			Self::Rename(a) => a.position.shared.clone(),
			Self::CodeAction(a) => a.position.shared.clone(),
			Self::Formatting(a) => a.shared.clone(),
			Self::Symbols(a) | Self::Diagnostics(a) => a.shared.clone(),
			Self::WorkspaceSymbols(a) => SharedArgs {
				lsp: Some(a.lsp.clone()),
				root: a.root.clone(),
				no_start_daemon: a.no_start_daemon,
				output: a.output,
			},
		}
	}

	/// Return the file path argument if the method has one.
	pub fn file_path(&self) -> Option<&Path> {
		match self {
			Self::Hover(a)
			| Self::Definition(a)
			| Self::Completion(a)
			| Self::Implementation(a)
			| Self::TypeDefinition(a)
			| Self::SignatureHelp(a) => Some(&a.file),
			Self::References(a) => Some(&a.position.file),
			Self::Rename(a) => Some(&a.position.file),
			Self::CodeAction(a) => Some(&a.position.file),
			Self::Formatting(a) => Some(&a.file),
			Self::Symbols(a) | Self::Diagnostics(a) => Some(&a.file),
			Self::WorkspaceSymbols(_) => None,
		}
	}

	/// Return the LSP protocol method name for this variant.
	pub fn lsp_method_name(&self) -> &'static str {
		match self {
			Self::Hover(_) => "textDocument/hover",
			Self::Definition(_) => "textDocument/definition",
			Self::References(_) => "textDocument/references",
			Self::Implementation(_) => "textDocument/implementation",
			Self::TypeDefinition(_) => "textDocument/typeDefinition",
			Self::Completion(_) => "textDocument/completion",
			Self::SignatureHelp(_) => "textDocument/signatureHelp",
			Self::Rename(_) => "textDocument/rename",
			Self::CodeAction(_) => "textDocument/codeAction",
			Self::Formatting(_) => "textDocument/formatting",
			Self::Symbols(_) => "textDocument/documentSymbol",
			Self::WorkspaceSymbols(_) => "workspace/symbol",
			Self::Diagnostics(_) => "textDocument/diagnostic",
		}
	}

	/// Whether this method returns location results that should be
	/// enriched with `context_line` fields.
	pub fn needs_location_enrichment(&self) -> bool {
		matches!(
			self,
			Self::Definition(_)
				| Self::References(_)
				| Self::Implementation(_)
				| Self::TypeDefinition(_)
		)
	}
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Convert a file path to a `file://` URI.
///
/// The path is canonicalized first so the URI always uses an absolute path
/// with symlinks resolved. Returns an error if the file does not exist or
/// the path cannot be converted to a URI.
pub fn file_uri(path: &Path) -> Result<String> {
	let canonical = path
		.canonicalize()
		.map_err(|e| anyhow::anyhow!("cannot resolve file path '{}': {e}", path.display()))?;
	let url = Url::from_file_path(&canonical)
		.map_err(|()| anyhow::anyhow!("cannot convert path to URI: {}", canonical.display()))?;
	Ok(url.into())
}

/// Parse a `file://` URI back into a filesystem path.
///
/// Returns `None` for non-file URIs, malformed URIs, or URIs that cannot
/// be converted to a local path.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
	let parsed = Url::parse(uri).ok()?;
	if parsed.scheme() != "file" {
		return None;
	}
	parsed.to_file_path().ok()
}

/// Resolve the LSP server identifier.
///
/// If `lsp` is `Some`, it is returned directly. Otherwise the server is
/// auto-resolved from the file extension using the built-in language
/// catalog (and any user/project config overrides). The first healthy
/// match is returned. Returns an error when no match is found.
pub fn resolve_lsp_id(lsp: Option<&str>, file: Option<&Path>) -> Result<String> {
	if let Some(id) = lsp {
		return Ok(id.to_string());
	}

	let file = file.ok_or_else(|| {
		anyhow::anyhow!(
			"--lsp is required when no --file is provided (needed for workspace-symbols)"
		)
	})?;

	let matches = languages::lsps_for_file(file, None, None)
		.map_err(|e| anyhow::anyhow!("failed to resolve LSP for file: {e}"))?;

	select_from_matches(&matches, file)
}

/// Pick the best LSP server from a list of candidates.
///
/// Prefers servers whose executable is found on `PATH`. Falls back to the
/// first match regardless of health so the daemon can produce a clearer
/// error message when it attempts to spawn the process.
fn select_from_matches(matches: &[languages::LspSelection], file: &Path) -> Result<String> {
	// Prefer the first server whose executable is on PATH.
	if let Some(healthy) = matches.iter().find(|m| m.executable_found) {
		return Ok(healthy.id.clone());
	}

	// Fall back to first match even if not healthy (the daemon will
	// produce a clearer error when it tries to spawn).
	if let Some(first) = matches.first() {
		return Ok(first.id.clone());
	}

	Err(anyhow::anyhow!(
		"no LSP server matches file '{}'; use --lsp to specify one explicitly",
		file.display()
	))
}

/// Read a single line from a file (zero-based line number).
///
/// Returns `None` if the file cannot be opened, the line number is out of
/// bounds, or any I/O error occurs.
pub fn read_context_line(path: &Path, line: u32) -> Option<String> {
	let file = File::open(path).ok()?;
	let reader = BufReader::new(file);
	reader.lines().nth(line as usize)?.ok()
}

/// Detect the LSP language identifier from a file extension.
///
/// Maps common file extensions to their LSP `languageId` string. Returns
/// `"plaintext"` for unrecognized extensions or files without an extension.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(language_id_for_extension("rs"), "rust");
/// assert_eq!(language_id_for_extension("tsx"), "typescriptreact");
/// assert_eq!(language_id_for_extension("unknown"), "plaintext");
/// ```
pub fn language_id_for_extension(ext: &str) -> &'static str {
	match ext {
		"rs" => "rust",
		"ts" => "typescript",
		"tsx" => "typescriptreact",
		"js" => "javascript",
		"jsx" => "javascriptreact",
		"py" => "python",
		"go" => "go",
		"toml" => "toml",
		"json" => "json",
		"md" => "markdown",
		"yaml" | "yml" => "yaml",
		"html" => "html",
		"css" => "css",
		_ => "plaintext",
	}
}

/// Detect the LSP language identifier for a file path.
///
/// Extracts the extension from the path and delegates to
/// [`language_id_for_extension`].
pub fn language_id_for_path(path: &Path) -> &'static str {
	path.extension()
		.and_then(|ext| ext.to_str())
		.map_or("plaintext", language_id_for_extension)
}

/// Extract the result payload from a JSON-RPC response.
///
/// Returns the value of the `result` field if present. If the response
/// contains an `error` field instead, that error object is returned
/// wrapped in `{"error": ...}`. If neither field is present, returns
/// `null`.
pub fn extract_lsp_result(response: &Value) -> Value {
	if let Some(result) = response.get("result") {
		return result.clone();
	}
	if let Some(error) = response.get("error") {
		return json!({ "error": error });
	}
	Value::Null
}

// ---------------------------------------------------------------------------
// Request building
// ---------------------------------------------------------------------------

/// Build the JSON-RPC request and metadata for the given method.
///
/// Returns a `(request_payload, meta)` tuple where `request_payload` is
/// the complete JSON-RPC 2.0 request object ready to send to the daemon,
/// and `meta` captures information needed to wrap the response.
pub fn build_request(method: &DoMethod) -> Result<(Value, RequestMeta)> {
	let lsp_method = method.lsp_method_name();

	match method {
		// -- Position-based methods -----------------------------------------
		DoMethod::Hover(args)
		| DoMethod::Definition(args)
		| DoMethod::Completion(args)
		| DoMethod::Implementation(args)
		| DoMethod::TypeDefinition(args)
		| DoMethod::SignatureHelp(args) => {
			let uri = file_uri(&args.file)?;
			let params = json!({
				"textDocument": { "uri": uri },
				"position": { "line": args.line, "character": args.col },
			});
			let meta = RequestMeta {
				lsp_method: lsp_method.to_string(),
				file_path: Some(args.file.clone()),
				position: Some(LspPosition {
					line: args.line,
					character: args.col,
				}),
			};
			Ok((jsonrpc_request(lsp_method, &params), meta))
		}

		// -- References (position + includeDeclaration) ---------------------
		DoMethod::References(args) => {
			let uri = file_uri(&args.position.file)?;
			let params = json!({
				"textDocument": { "uri": uri },
				"position": {
					"line": args.position.line,
					"character": args.position.col,
				},
				"context": {
					"includeDeclaration": args.include_declaration,
				},
			});
			let meta = RequestMeta {
				lsp_method: lsp_method.to_string(),
				file_path: Some(args.position.file.clone()),
				position: Some(LspPosition {
					line: args.position.line,
					character: args.position.col,
				}),
			};
			Ok((jsonrpc_request(lsp_method, &params), meta))
		}

		// -- Rename (position + newName) ------------------------------------
		DoMethod::Rename(args) => {
			let uri = file_uri(&args.position.file)?;
			let params = json!({
				"textDocument": { "uri": uri },
				"position": {
					"line": args.position.line,
					"character": args.position.col,
				},
				"newName": args.new_name,
			});
			let meta = RequestMeta {
				lsp_method: lsp_method.to_string(),
				file_path: Some(args.position.file.clone()),
				position: Some(LspPosition {
					line: args.position.line,
					character: args.position.col,
				}),
			};
			Ok((jsonrpc_request(lsp_method, &params), meta))
		}

		// -- Code action (range + context) ----------------------------------
		DoMethod::CodeAction(args) => {
			let uri = file_uri(&args.position.file)?;
			let end_line = args.end_line.unwrap_or(args.position.line);
			let end_col = args.end_col.unwrap_or(args.position.col);
			let params = json!({
				"textDocument": { "uri": uri },
				"range": {
					"start": {
						"line": args.position.line,
						"character": args.position.col,
					},
					"end": {
						"line": end_line,
						"character": end_col,
					},
				},
				"context": { "diagnostics": [] },
			});
			let meta = RequestMeta {
				lsp_method: lsp_method.to_string(),
				file_path: Some(args.position.file.clone()),
				position: Some(LspPosition {
					line: args.position.line,
					character: args.position.col,
				}),
			};
			Ok((jsonrpc_request(lsp_method, &params), meta))
		}

		// -- Formatting (file + options) ------------------------------------
		DoMethod::Formatting(args) => {
			let uri = file_uri(&args.file)?;
			let params = json!({
				"textDocument": { "uri": uri },
				"options": {
					"tabSize": args.tab_size,
					"insertSpaces": args.insert_spaces,
				},
			});
			let meta = RequestMeta {
				lsp_method: lsp_method.to_string(),
				file_path: Some(args.file.clone()),
				position: None,
			};
			Ok((jsonrpc_request(lsp_method, &params), meta))
		}

		// -- File-only methods (symbols, diagnostics) -----------------------
		DoMethod::Symbols(args) | DoMethod::Diagnostics(args) => {
			let uri = file_uri(&args.file)?;
			let params = json!({
				"textDocument": { "uri": uri },
			});
			let meta = RequestMeta {
				lsp_method: lsp_method.to_string(),
				file_path: Some(args.file.clone()),
				position: None,
			};
			Ok((jsonrpc_request(lsp_method, &params), meta))
		}

		// -- Workspace symbols (query only) ---------------------------------
		DoMethod::WorkspaceSymbols(args) => {
			let params = json!({
				"query": args.query,
			});
			let meta = RequestMeta {
				lsp_method: lsp_method.to_string(),
				file_path: None,
				position: None,
			};
			Ok((jsonrpc_request(lsp_method, &params), meta))
		}
	}
}

/// Build a JSON-RPC 2.0 request envelope.
fn jsonrpc_request(method: &str, params: &Value) -> Value {
	json!({
		"jsonrpc": "2.0",
		"id": 1,
		"method": method,
		"params": params,
	})
}

// ---------------------------------------------------------------------------
// Response wrapping
// ---------------------------------------------------------------------------

/// Wrap an LSP result with request metadata for agent consumption.
///
/// The response envelope includes the LSP server id, method name, file
/// path, position (when applicable), the raw result, and the elapsed
/// wall-clock time in milliseconds.
pub fn wrap_response(lsp_id: &str, meta: &RequestMeta, result: &Value, elapsed_ms: u64) -> Value {
	let mut obj = json!({
		"lsp_id": lsp_id,
		"method": meta.lsp_method,
		"result": result,
		"elapsed_ms": elapsed_ms,
	});

	if let Some(file) = &meta.file_path {
		obj["file"] = json!(file.display().to_string());
	}

	if let Some(pos) = &meta.position {
		obj["position"] = json!({
			"line": pos.line,
			"character": pos.character,
		});
	}

	obj
}

/// Enrich location results with `context_line` fields.
///
/// Walks the `result` value looking for LSP `Location` objects (with
/// `uri` + `range`) and `LocationLink` objects (with `targetUri` +
/// `targetRange`). For each location pointing to a local file, reads
/// the source line at `range.start.line` and inserts it as
/// `context_line`.
///
/// Handles single locations, arrays of locations, and mixed formats
/// gracefully. Non-file URIs and unreadable files are silently skipped
/// (the location is left without a `context_line`).
pub fn enrich_locations(result: &mut Value) {
	match result {
		Value::Array(arr) => {
			for item in arr.iter_mut() {
				enrich_single_location(item);
			}
		}
		Value::Object(_) => {
			enrich_single_location(result);
		}
		_ => {}
	}
}

/// Attempt to add `context_line` to a single location or location-link
/// object.
fn enrich_single_location(location: &mut Value) {
	// Try Location format: { uri, range }
	if let (Some(uri), Some(line)) = (
		location.get("uri").and_then(Value::as_str),
		location
			.get("range")
			.and_then(|r| r.get("start"))
			.and_then(|s| s.get("line"))
			.and_then(Value::as_u64),
	) {
		if let Some(path) = uri_to_path(uri) {
			if let Some(context) = read_context_line(&path, line as u32) {
				location["context_line"] = Value::String(context);
			}
		}
		return;
	}

	// Try LocationLink format: { targetUri, targetRange }
	if let (Some(uri), Some(line)) = (
		location.get("targetUri").and_then(Value::as_str),
		location
			.get("targetRange")
			.and_then(|r| r.get("start"))
			.and_then(|s| s.get("line"))
			.and_then(Value::as_u64),
	) {
		if let Some(path) = uri_to_path(uri) {
			if let Some(context) = read_context_line(&path, line as u32) {
				location["context_line"] = Value::String(context);
			}
		}
	}
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

/// Process the raw daemon response into a formatted output string.
///
/// Extracts the LSP result from the `CallOk` response, optionally enriches
/// location results with `context_line` fields, wraps everything with
/// metadata, and serializes to JSON according to the output format.
pub fn format_do_output(
	method: &DoMethod,
	lsp_id: &str,
	meta: &RequestMeta,
	call_ok: &CallOk,
	elapsed_ms: u64,
	output: DoOutput,
) -> Result<String> {
	let mut lsp_result = extract_lsp_result(&call_ok.response);
	if method.needs_location_enrichment() {
		enrich_locations(&mut lsp_result);
	}

	let wrapped = wrap_response(lsp_id, meta, &lsp_result, elapsed_ms);
	match output {
		DoOutput::Json => serde_json::to_string(&wrapped).map_err(Into::into),
		DoOutput::Pretty => serde_json::to_string_pretty(&wrapped).map_err(Into::into),
	}
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Run the `lspee do` command synchronously.
pub fn run(cmd: DoCommand) -> Result<()> {
	let runtime = tokio::runtime::Runtime::new()?;
	runtime.block_on(run_async(cmd))
}

/// Async implementation of the `lspee do` command.
///
/// Follows the same attach → call → release lifecycle as `lspee call`,
/// but builds the JSON-RPC request from structured arguments and wraps
/// the response with metadata.
async fn run_async(cmd: DoCommand) -> Result<()> {
	let shared = cmd.method.shared_args();

	// Resolve config.
	let resolved = lspee_config::resolve(shared.root.as_deref())?;

	// Resolve LSP server id.
	let lsp_id = resolve_lsp_id(shared.lsp.as_deref(), cmd.method.file_path())?;

	// Build JSON-RPC request.
	let (request_payload, meta) = build_request(&cmd.method)?;

	// Connect to daemon.
	let stream = client::connect(&resolved.project_root, !shared.no_start_daemon).await?;
	let (reader, mut writer) = stream.into_split();
	let mut lines = tokio::io::BufReader::new(reader).lines();

	// Attach session.
	let lease_id = attach_session(
		&mut writer,
		&mut lines,
		&resolved.project_root,
		&resolved.config_hash,
		&lsp_id,
	)
	.await?;

	// Send textDocument/didOpen if the method targets a file.
	let did_open_uri = if let Some(file_path) = cmd.method.file_path() {
		match send_did_open(&mut writer, &mut lines, &lease_id, file_path).await {
			Ok(uri) => Some(uri),
			Err(error) => {
				tracing::warn!(?error, "failed to send textDocument/didOpen");
				None
			}
		}
	} else {
		None
	};

	// Call LSP method (timed).
	let start = Instant::now();
	let call_id = client::new_request_id("call");
	let call = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(call_id.clone()),
		message_type: TYPE_CALL.to_string(),
		payload: serde_json::to_value(Call {
			lease_id: lease_id.clone(),
			request: request_payload,
		})?,
	};
	client::write_frame(&mut writer, &call).await?;
	let call_response = client::read_response_for_id(&mut lines, &call_id).await;

	// Send textDocument/didClose if we opened the document.
	if let Some(ref uri) = did_open_uri {
		if let Err(error) = send_did_close(&mut writer, &mut lines, &lease_id, uri).await {
			tracing::warn!(?error, "failed to send textDocument/didClose");
		}
	}

	// Always release lease.
	let release_result = release_lease(&mut writer, &mut lines, &lease_id).await;
	if let Err(error) = release_result {
		tracing::warn!(?error, lease_id, "failed to release lease after do call");
	}

	let call_response = call_response?;
	client::ensure_not_error(&call_response)?;
	if call_response.message_type != TYPE_CALL_OK {
		anyhow::bail!(
			"unexpected response type for Call: {}",
			call_response.message_type
		);
	}

	let elapsed_ms = start.elapsed().as_millis() as u64;
	let call_ok: CallOk = serde_json::from_value(call_response.payload)
		.map_err(|e| anyhow::anyhow!("invalid CallOk payload: {e}"))?;

	let formatted = format_do_output(
		&cmd.method,
		&lsp_id,
		&meta,
		&call_ok,
		elapsed_ms,
		shared.output,
	)?;
	println!("{formatted}");

	Ok(())
}

/// Attach to a daemon session, returning the lease id.
async fn attach_session(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut Lines<tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>>,
	project_root: &Path,
	config_hash: &str,
	lsp_id: &str,
) -> Result<String> {
	let attach_id = client::new_request_id("attach");
	let attach = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(attach_id.clone()),
		message_type: TYPE_ATTACH.to_string(),
		payload: serde_json::to_value(Attach {
			session_key: SessionKeyWire {
				project_root: project_root.display().to_string(),
				config_hash: config_hash.to_string(),
				lsp_id: lsp_id.to_string(),
			},
			client_meta: ClientMeta {
				client_name: "lspee_cli".to_string(),
				client_version: env!("CARGO_PKG_VERSION").to_string(),
				client_kind: Some(ClientKind::Agent),
				pid: Some(process::id()),
				cwd: std::env::current_dir()
					.ok()
					.map(|cwd| cwd.display().to_string()),
			},
			capabilities: Some(AttachCapabilities {
				stream_mode: vec![StreamMode::MuxControl],
			}),
		})?,
	};

	client::write_frame(writer, &attach).await?;
	let response = client::read_response_for_id(lines, &attach_id).await?;
	client::ensure_not_error(&response)?;
	if response.message_type != TYPE_ATTACH_OK {
		anyhow::bail!(
			"unexpected response type for Attach: {}",
			response.message_type
		);
	}

	response
		.payload
		.get("lease_id")
		.and_then(Value::as_str)
		.map(String::from)
		.ok_or_else(|| anyhow::anyhow!("AttachOk missing lease_id"))
}

/// Release a daemon lease.
async fn release_lease(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut Lines<tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>>,
	lease_id: &str,
) -> Result<()> {
	let release_id = client::new_request_id("release");
	let release = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(release_id.clone()),
		message_type: TYPE_RELEASE.to_string(),
		payload: serde_json::to_value(Release {
			lease_id: lease_id.to_string(),
			reason: None,
		})?,
	};

	client::write_frame(writer, &release).await?;
	let response = client::read_response_for_id(lines, &release_id).await?;
	client::ensure_not_error(&response)?;
	if response.message_type != TYPE_RELEASE_OK {
		anyhow::bail!(
			"unexpected response type for Release: {}",
			response.message_type
		);
	}

	Ok(())
}

/// Send a `textDocument/didOpen` notification via the daemon's `Notify`
/// protocol. Returns the file URI on success so it can be reused for
/// `didClose`.
async fn send_did_open(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut Lines<tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>>,
	lease_id: &str,
	file_path: &Path,
) -> Result<String> {
	let uri = file_uri(file_path)?;
	let language_id = language_id_for_path(file_path);
	let text = std::fs::read_to_string(file_path).map_err(|e| {
		anyhow::anyhow!(
			"cannot read file '{}' for didOpen: {e}",
			file_path.display()
		)
	})?;

	let message = json!({
		"jsonrpc": "2.0",
		"method": "textDocument/didOpen",
		"params": {
			"textDocument": {
				"uri": uri,
				"languageId": language_id,
				"version": 1,
				"text": text,
			}
		}
	});

	let notify_id = client::new_request_id("notify-open");
	let envelope = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(notify_id.clone()),
		message_type: TYPE_NOTIFY.to_string(),
		payload: serde_json::to_value(Notify {
			lease_id: lease_id.to_string(),
			message,
		})?,
	};

	client::write_frame(writer, &envelope).await?;
	let response = client::read_response_for_id(lines, &notify_id).await?;
	client::ensure_not_error(&response)?;
	if response.message_type != TYPE_NOTIFY_OK {
		anyhow::bail!(
			"unexpected response type for Notify (didOpen): {}",
			response.message_type
		);
	}

	Ok(uri)
}

/// Send a `textDocument/didClose` notification via the daemon's `Notify`
/// protocol.
async fn send_did_close(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut Lines<tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>>,
	lease_id: &str,
	uri: &str,
) -> Result<()> {
	let message = json!({
		"jsonrpc": "2.0",
		"method": "textDocument/didClose",
		"params": {
			"textDocument": {
				"uri": uri,
			}
		}
	});

	let notify_id = client::new_request_id("notify-close");
	let envelope = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(notify_id.clone()),
		message_type: TYPE_NOTIFY.to_string(),
		payload: serde_json::to_value(Notify {
			lease_id: lease_id.to_string(),
			message,
		})?,
	};

	client::write_frame(writer, &envelope).await?;
	let response = client::read_response_for_id(lines, &notify_id).await?;
	client::ensure_not_error(&response)?;
	if response.message_type != TYPE_NOTIFY_OK {
		anyhow::bail!(
			"unexpected response type for Notify (didClose): {}",
			response.message_type
		);
	}

	Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use std::fs;
	use std::io::Write as _;
	use std::time::SystemTime;
	use std::time::UNIX_EPOCH;

	use serde_json::json;

	use super::*;

	/// Create a unique temporary directory for test isolation.
	fn unique_temp_dir(name: &str) -> PathBuf {
		let nanos = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("time should be after epoch")
			.as_nanos();
		let dir = std::env::temp_dir().join(format!("lspee_do_test_{name}_{nanos}"));
		fs::create_dir_all(&dir).expect("should create temp dir");
		dir.canonicalize().expect("should canonicalize temp dir")
	}

	/// Write content to a file in a temp directory.
	fn write_temp_file(dir: &Path, name: &str, content: &str) -> PathBuf {
		let path = dir.join(name);
		let mut f = File::create(&path).expect("should create temp file");
		f.write_all(content.as_bytes())
			.expect("should write temp file");
		path
	}

	// -- file_uri -----------------------------------------------------------

	#[test]
	fn file_uri_for_existing_file() {
		let dir = unique_temp_dir("file_uri");
		let path = write_temp_file(&dir, "test.rs", "fn main() {}");
		let uri = file_uri(&path).unwrap();
		assert!(
			uri.starts_with("file:///"),
			"URI should start with file:///"
		);
		assert!(uri.ends_with("test.rs"), "URI should end with test.rs");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn file_uri_for_nonexistent_file() {
		let result = file_uri(Path::new("/nonexistent/path/to/file.rs"));
		assert!(result.is_err(), "should error for nonexistent file");
		let err = result.unwrap_err().to_string();
		assert!(
			err.contains("cannot resolve file path"),
			"error should mention path resolution: {err}"
		);
	}

	#[test]
	fn file_uri_resolves_relative_path() {
		let dir = unique_temp_dir("file_uri_rel");
		let file = write_temp_file(&dir, "hello.rs", "hello");
		// Construct a relative-looking path by going through the parent.
		let parent = dir.parent().unwrap();
		let dir_name = dir.file_name().unwrap();
		let relative_ish = parent.join(dir_name).join("hello.rs");
		let uri = file_uri(&relative_ish).unwrap();
		// Should produce the same canonical URI.
		let expected = file_uri(&file).unwrap();
		assert_eq!(uri, expected);
		fs::remove_dir_all(&dir).ok();
	}

	// -- uri_to_path --------------------------------------------------------

	#[test]
	fn uri_to_path_valid_file_uri() {
		let path = uri_to_path("file:///tmp/test.rs");
		assert_eq!(path, Some(PathBuf::from("/tmp/test.rs")));
	}

	#[test]
	fn uri_to_path_non_file_scheme() {
		assert_eq!(uri_to_path("https://example.com/file.rs"), None);
	}

	#[test]
	fn uri_to_path_untitled_scheme() {
		assert_eq!(uri_to_path("untitled:Untitled-1"), None);
	}

	#[test]
	fn uri_to_path_malformed_uri() {
		assert_eq!(uri_to_path("not a uri at all"), None);
	}

	#[test]
	fn uri_to_path_empty_string() {
		assert_eq!(uri_to_path(""), None);
	}

	// -- resolve_lsp_id -----------------------------------------------------

	#[test]
	fn resolve_lsp_id_explicit_passthrough() {
		let result = resolve_lsp_id(Some("custom-lsp"), None).unwrap();
		assert_eq!(result, "custom-lsp");
	}

	#[test]
	fn resolve_lsp_id_explicit_overrides_file() {
		let dir = unique_temp_dir("resolve_explicit");
		let file = write_temp_file(&dir, "main.rs", "");
		let result = resolve_lsp_id(Some("custom-lsp"), Some(&file)).unwrap();
		assert_eq!(result, "custom-lsp");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn resolve_lsp_id_auto_from_rs_file() {
		let dir = unique_temp_dir("resolve_rs");
		let file = write_temp_file(&dir, "main.rs", "");
		let result = resolve_lsp_id(None, Some(&file)).unwrap();
		assert_eq!(result, "rust-analyzer");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn resolve_lsp_id_auto_from_ts_file() {
		let dir = unique_temp_dir("resolve_ts");
		let file = write_temp_file(&dir, "index.ts", "");
		let result = resolve_lsp_id(None, Some(&file)).unwrap();
		// Should match typescript-language-server or vtsls from catalog.
		assert!(!result.is_empty(), "should resolve to some typescript LSP");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn resolve_lsp_id_no_match_for_unknown_extension() {
		let dir = unique_temp_dir("resolve_unknown");
		let file = write_temp_file(&dir, "data.zzzzz", "");
		let result = resolve_lsp_id(None, Some(&file));
		assert!(result.is_err(), "should error for unknown extension");
		let err = result.unwrap_err().to_string();
		assert!(
			err.contains("no LSP server matches"),
			"error should mention no match: {err}"
		);
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn resolve_lsp_id_no_file_no_lsp() {
		let result = resolve_lsp_id(None, None);
		assert!(
			result.is_err(),
			"should error when both lsp and file are None"
		);
		let err = result.unwrap_err().to_string();
		assert!(
			err.contains("--lsp is required"),
			"error should mention --lsp: {err}"
		);
	}

	// -- read_context_line --------------------------------------------------

	#[test]
	fn read_context_line_first_line() {
		let dir = unique_temp_dir("ctx_first");
		let file = write_temp_file(&dir, "test.rs", "line zero\nline one\nline two\n");
		assert_eq!(read_context_line(&file, 0), Some("line zero".to_string()));
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn read_context_line_middle_line() {
		let dir = unique_temp_dir("ctx_mid");
		let file = write_temp_file(&dir, "test.rs", "line zero\nline one\nline two\n");
		assert_eq!(read_context_line(&file, 1), Some("line one".to_string()));
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn read_context_line_last_line() {
		let dir = unique_temp_dir("ctx_last");
		let file = write_temp_file(&dir, "test.rs", "line zero\nline one\nline two");
		assert_eq!(read_context_line(&file, 2), Some("line two".to_string()));
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn read_context_line_out_of_bounds() {
		let dir = unique_temp_dir("ctx_oob");
		let file = write_temp_file(&dir, "test.rs", "only one line");
		assert_eq!(read_context_line(&file, 5), None);
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn read_context_line_nonexistent_file() {
		assert_eq!(read_context_line(Path::new("/no/such/file.rs"), 0), None);
	}

	#[test]
	fn read_context_line_empty_file() {
		let dir = unique_temp_dir("ctx_empty");
		let file = write_temp_file(&dir, "empty.rs", "");
		assert_eq!(read_context_line(&file, 0), None);
		fs::remove_dir_all(&dir).ok();
	}

	// -- extract_lsp_result -------------------------------------------------

	#[test]
	fn extract_lsp_result_with_result_field() {
		let response = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"result": { "contents": "hello" }
		});
		let result = extract_lsp_result(&response);
		assert_eq!(result, json!({ "contents": "hello" }));
	}

	#[test]
	fn extract_lsp_result_with_error_field() {
		let response = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"error": { "code": -32601, "message": "Method not found" }
		});
		let result = extract_lsp_result(&response);
		assert_eq!(
			result,
			json!({ "error": { "code": -32601, "message": "Method not found" } })
		);
	}

	#[test]
	fn extract_lsp_result_with_neither() {
		let response = json!({ "jsonrpc": "2.0", "id": 1 });
		let result = extract_lsp_result(&response);
		assert_eq!(result, Value::Null);
	}

	#[test]
	fn extract_lsp_result_null_result() {
		let response = json!({ "jsonrpc": "2.0", "id": 1, "result": null });
		let result = extract_lsp_result(&response);
		assert_eq!(result, Value::Null);
	}

	// -- needs_location_enrichment ------------------------------------------

	#[test]
	fn needs_enrichment_for_location_methods() {
		let dir = unique_temp_dir("enrich_check");
		let file = write_temp_file(&dir, "t.rs", "");
		let pos = PositionArgs {
			shared: SharedArgs {
				lsp: None,
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
			},
			file: file.clone(),
			line: 0,
			col: 0,
		};

		assert!(DoMethod::Definition(pos.clone()).needs_location_enrichment());
		assert!(DoMethod::Implementation(pos.clone()).needs_location_enrichment());
		assert!(DoMethod::TypeDefinition(pos.clone()).needs_location_enrichment());
		assert!(
			DoMethod::References(ReferencesArgs {
				position: pos.clone(),
				include_declaration: false,
			})
			.needs_location_enrichment()
		);

		assert!(!DoMethod::Hover(pos.clone()).needs_location_enrichment());
		assert!(!DoMethod::Completion(pos.clone()).needs_location_enrichment());
		assert!(!DoMethod::SignatureHelp(pos).needs_location_enrichment());
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn needs_enrichment_false_for_non_location_methods() {
		let file_args = FileOnlyArgs {
			shared: SharedArgs {
				lsp: None,
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
			},
			file: PathBuf::from("/tmp/test.rs"),
		};
		let ws_args = WorkspaceSymbolArgs {
			lsp: "test".to_string(),
			root: None,
			no_start_daemon: false,
			output: DoOutput::Json,
			query: "test".to_string(),
		};

		assert!(!DoMethod::Symbols(file_args.clone()).needs_location_enrichment());
		assert!(!DoMethod::Diagnostics(file_args).needs_location_enrichment());
		assert!(!DoMethod::WorkspaceSymbols(ws_args).needs_location_enrichment());
	}

	// -- lsp_method_name ----------------------------------------------------

	#[test]
	fn lsp_method_names_are_correct() {
		let dir = unique_temp_dir("method_names");
		let file = write_temp_file(&dir, "t.rs", "");
		let pos = PositionArgs {
			shared: SharedArgs {
				lsp: None,
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
			},
			file: file.clone(),
			line: 0,
			col: 0,
		};
		let file_args = FileOnlyArgs {
			shared: SharedArgs {
				lsp: None,
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
			},
			file: file.clone(),
		};
		let fmt_args = FormattingArgs {
			shared: SharedArgs {
				lsp: None,
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
			},
			file,
			tab_size: 4,
			insert_spaces: true,
		};

		assert_eq!(
			DoMethod::Hover(pos.clone()).lsp_method_name(),
			"textDocument/hover"
		);
		assert_eq!(
			DoMethod::Definition(pos.clone()).lsp_method_name(),
			"textDocument/definition"
		);
		assert_eq!(
			DoMethod::References(ReferencesArgs {
				position: pos.clone(),
				include_declaration: false,
			})
			.lsp_method_name(),
			"textDocument/references"
		);
		assert_eq!(
			DoMethod::Implementation(pos.clone()).lsp_method_name(),
			"textDocument/implementation"
		);
		assert_eq!(
			DoMethod::TypeDefinition(pos.clone()).lsp_method_name(),
			"textDocument/typeDefinition"
		);
		assert_eq!(
			DoMethod::Completion(pos.clone()).lsp_method_name(),
			"textDocument/completion"
		);
		assert_eq!(
			DoMethod::SignatureHelp(pos.clone()).lsp_method_name(),
			"textDocument/signatureHelp"
		);
		assert_eq!(
			DoMethod::Rename(RenameArgs {
				position: pos.clone(),
				new_name: "x".to_string(),
			})
			.lsp_method_name(),
			"textDocument/rename"
		);
		assert_eq!(
			DoMethod::CodeAction(CodeActionArgs {
				position: pos,
				end_line: None,
				end_col: None,
			})
			.lsp_method_name(),
			"textDocument/codeAction"
		);
		assert_eq!(
			DoMethod::Formatting(fmt_args).lsp_method_name(),
			"textDocument/formatting"
		);
		assert_eq!(
			DoMethod::Symbols(file_args.clone()).lsp_method_name(),
			"textDocument/documentSymbol"
		);
		assert_eq!(
			DoMethod::Diagnostics(file_args).lsp_method_name(),
			"textDocument/diagnostic"
		);
		assert_eq!(
			DoMethod::WorkspaceSymbols(WorkspaceSymbolArgs {
				lsp: "t".to_string(),
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
				query: "q".to_string(),
			})
			.lsp_method_name(),
			"workspace/symbol"
		);
		fs::remove_dir_all(&dir).ok();
	}

	// -- shared_args --------------------------------------------------------

	/// Helper: assert `shared_args()` returns the expected LSP id.
	fn assert_shared_lsp(method: &DoMethod, expected: &str) {
		let s = method.shared_args();
		assert_eq!(s.lsp.as_deref(), Some(expected));
	}

	#[test]
	fn shared_args_extracted_from_every_position_variant() {
		let dir = unique_temp_dir("sa_pos");
		let file = write_temp_file(&dir, "t.rs", "");
		let shared = SharedArgs {
			lsp: Some("x".to_string()),
			root: None,
			no_start_daemon: false,
			output: DoOutput::Json,
		};
		let pos = PositionArgs {
			shared,
			file,
			line: 0,
			col: 0,
		};

		// Each arm of the or-pattern must be individually hit.
		assert_shared_lsp(&DoMethod::Hover(pos.clone()), "x");
		assert_shared_lsp(&DoMethod::Definition(pos.clone()), "x");
		assert_shared_lsp(&DoMethod::Completion(pos.clone()), "x");
		assert_shared_lsp(&DoMethod::Implementation(pos.clone()), "x");
		assert_shared_lsp(&DoMethod::TypeDefinition(pos.clone()), "x");
		assert_shared_lsp(&DoMethod::SignatureHelp(pos), "x");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn shared_args_extracted_from_composite_variants() {
		let dir = unique_temp_dir("sa_comp");
		let file = write_temp_file(&dir, "t.rs", "");
		let shared = SharedArgs {
			lsp: Some("y".to_string()),
			root: None,
			no_start_daemon: true,
			output: DoOutput::Pretty,
		};
		let pos = PositionArgs {
			shared: shared.clone(),
			file: file.clone(),
			line: 0,
			col: 0,
		};

		assert_shared_lsp(
			&DoMethod::References(ReferencesArgs {
				position: pos.clone(),
				include_declaration: false,
			}),
			"y",
		);
		assert_shared_lsp(
			&DoMethod::Rename(RenameArgs {
				position: pos.clone(),
				new_name: "n".to_string(),
			}),
			"y",
		);
		assert_shared_lsp(
			&DoMethod::CodeAction(CodeActionArgs {
				position: pos,
				end_line: None,
				end_col: None,
			}),
			"y",
		);
		assert_shared_lsp(
			&DoMethod::Formatting(FormattingArgs {
				shared: shared.clone(),
				file: file.clone(),
				tab_size: 2,
				insert_spaces: false,
			}),
			"y",
		);
		assert_shared_lsp(
			&DoMethod::Symbols(FileOnlyArgs {
				shared: shared.clone(),
				file: file.clone(),
			}),
			"y",
		);
		assert_shared_lsp(&DoMethod::Diagnostics(FileOnlyArgs { shared, file }), "y");

		assert_shared_lsp(
			&DoMethod::WorkspaceSymbols(WorkspaceSymbolArgs {
				lsp: "ws".to_string(),
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
				query: "q".to_string(),
			}),
			"ws",
		);
		fs::remove_dir_all(&dir).ok();
	}

	// -- file_path accessor -------------------------------------------------

	#[test]
	fn file_path_returns_path_for_every_file_variant() {
		let dir = unique_temp_dir("fp_all");
		let file = write_temp_file(&dir, "t.rs", "");
		let shared = SharedArgs {
			lsp: None,
			root: None,
			no_start_daemon: false,
			output: DoOutput::Json,
		};
		let pos = PositionArgs {
			shared: shared.clone(),
			file: file.clone(),
			line: 0,
			col: 0,
		};
		let file_args = FileOnlyArgs {
			shared: shared.clone(),
			file: file.clone(),
		};

		// Position-based: each or-pattern arm.
		assert_eq!(
			DoMethod::Hover(pos.clone()).file_path(),
			Some(file.as_path())
		);
		assert_eq!(
			DoMethod::Definition(pos.clone()).file_path(),
			Some(file.as_path())
		);
		assert_eq!(
			DoMethod::Completion(pos.clone()).file_path(),
			Some(file.as_path())
		);
		assert_eq!(
			DoMethod::Implementation(pos.clone()).file_path(),
			Some(file.as_path())
		);
		assert_eq!(
			DoMethod::TypeDefinition(pos.clone()).file_path(),
			Some(file.as_path())
		);
		assert_eq!(
			DoMethod::SignatureHelp(pos.clone()).file_path(),
			Some(file.as_path())
		);

		// Composite variants.
		assert_eq!(
			DoMethod::References(ReferencesArgs {
				position: pos.clone(),
				include_declaration: false
			})
			.file_path(),
			Some(file.as_path())
		);
		assert_eq!(
			DoMethod::Rename(RenameArgs {
				position: pos.clone(),
				new_name: "n".to_string()
			})
			.file_path(),
			Some(file.as_path())
		);
		assert_eq!(
			DoMethod::CodeAction(CodeActionArgs {
				position: pos,
				end_line: None,
				end_col: None
			})
			.file_path(),
			Some(file.as_path())
		);
		assert_eq!(
			DoMethod::Formatting(FormattingArgs {
				shared,
				file: file.clone(),
				tab_size: 4,
				insert_spaces: true
			})
			.file_path(),
			Some(file.as_path())
		);
		assert_eq!(
			DoMethod::Symbols(file_args.clone()).file_path(),
			Some(file.as_path())
		);
		assert_eq!(
			DoMethod::Diagnostics(file_args).file_path(),
			Some(file.as_path())
		);

		// No file.
		assert!(
			DoMethod::WorkspaceSymbols(WorkspaceSymbolArgs {
				lsp: "t".to_string(),
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
				query: "q".to_string(),
			})
			.file_path()
			.is_none()
		);

		fs::remove_dir_all(&dir).ok();
	}

	// -- select_from_matches ------------------------------------------------

	#[test]
	fn select_from_matches_prefers_healthy() {
		let matches = vec![
			languages::LspSelection {
				id: "unhealthy".to_string(),
				command: "no-such-cmd".to_string(),
				args: vec![],
				root_markers: vec![],
				executable_found: false,
			},
			languages::LspSelection {
				id: "healthy".to_string(),
				command: "cat".to_string(),
				args: vec![],
				root_markers: vec![],
				executable_found: true,
			},
		];
		let result = select_from_matches(&matches, Path::new("test.rs")).unwrap();
		assert_eq!(result, "healthy");
	}

	#[test]
	fn select_from_matches_falls_back_to_unhealthy() {
		let matches = vec![languages::LspSelection {
			id: "only-option".to_string(),
			command: "no-such-cmd".to_string(),
			args: vec![],
			root_markers: vec![],
			executable_found: false,
		}];
		let result = select_from_matches(&matches, Path::new("test.rs")).unwrap();
		assert_eq!(result, "only-option");
	}

	#[test]
	fn select_from_matches_empty_returns_error() {
		let result = select_from_matches(&[], Path::new("test.xyz"));
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert!(err.contains("no LSP server matches"));
	}

	// -- build_request (one test per method) --------------------------------

	fn make_position_args(file: &Path) -> PositionArgs {
		PositionArgs {
			shared: SharedArgs {
				lsp: Some("test-lsp".to_string()),
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
			},
			file: file.to_path_buf(),
			line: 10,
			col: 5,
		}
	}

	#[test]
	fn build_request_hover() {
		let dir = unique_temp_dir("br_hover");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = make_position_args(&file);
		let (req, meta) = build_request(&DoMethod::Hover(args)).unwrap();
		assert_eq!(req["method"], "textDocument/hover");
		assert_eq!(req["params"]["position"]["line"], 10);
		assert_eq!(req["params"]["position"]["character"], 5);
		assert!(
			req["params"]["textDocument"]["uri"]
				.as_str()
				.unwrap()
				.ends_with("t.rs")
		);
		assert_eq!(meta.lsp_method, "textDocument/hover");
		assert!(meta.position.is_some());
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_definition() {
		let dir = unique_temp_dir("br_def");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = make_position_args(&file);
		let (req, meta) = build_request(&DoMethod::Definition(args)).unwrap();
		assert_eq!(req["method"], "textDocument/definition");
		assert_eq!(meta.lsp_method, "textDocument/definition");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_references() {
		let dir = unique_temp_dir("br_ref");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = ReferencesArgs {
			position: make_position_args(&file),
			include_declaration: true,
		};
		let (req, meta) = build_request(&DoMethod::References(args)).unwrap();
		assert_eq!(req["method"], "textDocument/references");
		assert_eq!(req["params"]["context"]["includeDeclaration"], true);
		assert_eq!(meta.lsp_method, "textDocument/references");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_references_no_declaration() {
		let dir = unique_temp_dir("br_ref_no");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = ReferencesArgs {
			position: make_position_args(&file),
			include_declaration: false,
		};
		let (req, _) = build_request(&DoMethod::References(args)).unwrap();
		assert_eq!(req["params"]["context"]["includeDeclaration"], false);
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_implementation() {
		let dir = unique_temp_dir("br_impl");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = make_position_args(&file);
		let (req, _) = build_request(&DoMethod::Implementation(args)).unwrap();
		assert_eq!(req["method"], "textDocument/implementation");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_type_definition() {
		let dir = unique_temp_dir("br_typedef");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = make_position_args(&file);
		let (req, _) = build_request(&DoMethod::TypeDefinition(args)).unwrap();
		assert_eq!(req["method"], "textDocument/typeDefinition");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_completion() {
		let dir = unique_temp_dir("br_comp");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = make_position_args(&file);
		let (req, _) = build_request(&DoMethod::Completion(args)).unwrap();
		assert_eq!(req["method"], "textDocument/completion");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_signature_help() {
		let dir = unique_temp_dir("br_sig");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = make_position_args(&file);
		let (req, _) = build_request(&DoMethod::SignatureHelp(args)).unwrap();
		assert_eq!(req["method"], "textDocument/signatureHelp");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_rename() {
		let dir = unique_temp_dir("br_rename");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = RenameArgs {
			position: make_position_args(&file),
			new_name: "new_symbol".to_string(),
		};
		let (req, _) = build_request(&DoMethod::Rename(args)).unwrap();
		assert_eq!(req["method"], "textDocument/rename");
		assert_eq!(req["params"]["newName"], "new_symbol");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_code_action_point() {
		let dir = unique_temp_dir("br_ca_pt");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = CodeActionArgs {
			position: make_position_args(&file),
			end_line: None,
			end_col: None,
		};
		let (req, _) = build_request(&DoMethod::CodeAction(args)).unwrap();
		assert_eq!(req["method"], "textDocument/codeAction");
		assert_eq!(req["params"]["range"]["start"]["line"], 10);
		assert_eq!(req["params"]["range"]["end"]["line"], 10);
		assert_eq!(req["params"]["range"]["end"]["character"], 5);
		assert_eq!(req["params"]["context"]["diagnostics"], json!([]));
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_code_action_range() {
		let dir = unique_temp_dir("br_ca_rng");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = CodeActionArgs {
			position: make_position_args(&file),
			end_line: Some(20),
			end_col: Some(15),
		};
		let (req, _) = build_request(&DoMethod::CodeAction(args)).unwrap();
		assert_eq!(req["params"]["range"]["start"]["line"], 10);
		assert_eq!(req["params"]["range"]["start"]["character"], 5);
		assert_eq!(req["params"]["range"]["end"]["line"], 20);
		assert_eq!(req["params"]["range"]["end"]["character"], 15);
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_formatting() {
		let dir = unique_temp_dir("br_fmt");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = FormattingArgs {
			shared: SharedArgs {
				lsp: None,
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
			},
			file,
			tab_size: 2,
			insert_spaces: false,
		};
		let (req, meta) = build_request(&DoMethod::Formatting(args)).unwrap();
		assert_eq!(req["method"], "textDocument/formatting");
		assert_eq!(req["params"]["options"]["tabSize"], 2);
		assert_eq!(req["params"]["options"]["insertSpaces"], false);
		assert!(meta.position.is_none());
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_symbols() {
		let dir = unique_temp_dir("br_sym");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = FileOnlyArgs {
			shared: SharedArgs {
				lsp: None,
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
			},
			file,
		};
		let (req, meta) = build_request(&DoMethod::Symbols(args)).unwrap();
		assert_eq!(req["method"], "textDocument/documentSymbol");
		assert!(meta.position.is_none());
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_diagnostics() {
		let dir = unique_temp_dir("br_diag");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = FileOnlyArgs {
			shared: SharedArgs {
				lsp: None,
				root: None,
				no_start_daemon: false,
				output: DoOutput::Json,
			},
			file,
		};
		let (req, _) = build_request(&DoMethod::Diagnostics(args)).unwrap();
		assert_eq!(req["method"], "textDocument/diagnostic");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn build_request_workspace_symbols() {
		let args = WorkspaceSymbolArgs {
			lsp: "test-lsp".to_string(),
			root: None,
			no_start_daemon: false,
			output: DoOutput::Json,
			query: "MyStruct".to_string(),
		};
		let (req, meta) = build_request(&DoMethod::WorkspaceSymbols(args)).unwrap();
		assert_eq!(req["method"], "workspace/symbol");
		assert_eq!(req["params"]["query"], "MyStruct");
		assert!(meta.file_path.is_none());
		assert!(meta.position.is_none());
	}

	#[test]
	fn build_request_jsonrpc_envelope() {
		let dir = unique_temp_dir("br_envelope");
		let file = write_temp_file(&dir, "t.rs", "");
		let args = make_position_args(&file);
		let (req, _) = build_request(&DoMethod::Hover(args)).unwrap();
		assert_eq!(req["jsonrpc"], "2.0");
		assert_eq!(req["id"], 1);
		assert!(req.get("method").is_some());
		assert!(req.get("params").is_some());
		fs::remove_dir_all(&dir).ok();
	}

	// -- wrap_response ------------------------------------------------------

	#[test]
	fn wrap_response_with_file_and_position() {
		let meta = RequestMeta {
			lsp_method: "textDocument/hover".to_string(),
			file_path: Some(PathBuf::from("src/main.rs")),
			position: Some(LspPosition {
				line: 10,
				character: 5,
			}),
		};
		let result = json!({ "contents": "fn main()" });
		let wrapped = wrap_response("rust-analyzer", &meta, &result, 42);

		assert_eq!(wrapped["lsp_id"], "rust-analyzer");
		assert_eq!(wrapped["method"], "textDocument/hover");
		assert_eq!(wrapped["file"], "src/main.rs");
		assert_eq!(wrapped["position"]["line"], 10);
		assert_eq!(wrapped["position"]["character"], 5);
		assert_eq!(wrapped["result"]["contents"], "fn main()");
		assert_eq!(wrapped["elapsed_ms"], 42);
	}

	#[test]
	fn wrap_response_file_only() {
		let meta = RequestMeta {
			lsp_method: "textDocument/formatting".to_string(),
			file_path: Some(PathBuf::from("src/lib.rs")),
			position: None,
		};
		let result = json!([]);
		let wrapped = wrap_response("rust-analyzer", &meta, &result, 10);

		assert_eq!(wrapped["file"], "src/lib.rs");
		assert!(wrapped.get("position").is_none());
	}

	#[test]
	fn wrap_response_no_file_no_position() {
		let meta = RequestMeta {
			lsp_method: "workspace/symbol".to_string(),
			file_path: None,
			position: None,
		};
		let result = json!([]);
		let wrapped = wrap_response("test-lsp", &meta, &result, 100);

		assert!(wrapped.get("file").is_none());
		assert!(wrapped.get("position").is_none());
		assert_eq!(wrapped["elapsed_ms"], 100);
	}

	// -- enrich_locations ---------------------------------------------------

	#[test]
	fn enrich_locations_single_location() {
		let dir = unique_temp_dir("enrich_single");
		let file = write_temp_file(&dir, "lib.rs", "use std::io;\npub fn hello() {\n}\n");
		let uri = file_uri(&file).unwrap();
		let mut result = json!({
			"uri": uri,
			"range": {
				"start": { "line": 1, "character": 0 },
				"end": { "line": 1, "character": 14 },
			}
		});
		enrich_locations(&mut result);
		assert_eq!(result["context_line"], "pub fn hello() {");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn enrich_locations_array() {
		let dir = unique_temp_dir("enrich_arr");
		let file = write_temp_file(&dir, "lib.rs", "line0\nline1\nline2\n");
		let uri = file_uri(&file).unwrap();
		let mut result = json!([
			{ "uri": uri.clone(), "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } } },
			{ "uri": uri, "range": { "start": { "line": 2, "character": 0 }, "end": { "line": 2, "character": 5 } } },
		]);
		enrich_locations(&mut result);
		assert_eq!(result[0]["context_line"], "line0");
		assert_eq!(result[1]["context_line"], "line2");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn enrich_locations_location_link() {
		let dir = unique_temp_dir("enrich_link");
		let file = write_temp_file(&dir, "lib.rs", "struct Foo;\nimpl Foo {}\n");
		let uri = file_uri(&file).unwrap();
		let mut result = json!([{
			"targetUri": uri,
			"targetRange": {
				"start": { "line": 1, "character": 0 },
				"end": { "line": 1, "character": 11 },
			},
			"targetSelectionRange": {
				"start": { "line": 1, "character": 5 },
				"end": { "line": 1, "character": 8 },
			},
		}]);
		enrich_locations(&mut result);
		assert_eq!(result[0]["context_line"], "impl Foo {}");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn enrich_locations_non_file_uri() {
		let mut result = json!([{
			"uri": "untitled:Untitled-1",
			"range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } },
		}]);
		enrich_locations(&mut result);
		// No context_line added for non-file URIs.
		assert!(result[0].get("context_line").is_none());
	}

	#[test]
	fn enrich_locations_missing_file() {
		let mut result = json!([{
			"uri": "file:///nonexistent/path/test.rs",
			"range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } },
		}]);
		enrich_locations(&mut result);
		assert!(result[0].get("context_line").is_none());
	}

	#[test]
	fn enrich_locations_out_of_bounds_line() {
		let dir = unique_temp_dir("enrich_oob");
		let file = write_temp_file(&dir, "t.rs", "only one line");
		let uri = file_uri(&file).unwrap();
		let mut result = json!({
			"uri": uri,
			"range": { "start": { "line": 99, "character": 0 }, "end": { "line": 99, "character": 5 } },
		});
		enrich_locations(&mut result);
		assert!(result.get("context_line").is_none());
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn enrich_locations_location_link_non_file_uri() {
		let mut result = json!([{
			"targetUri": "untitled:Untitled-1",
			"targetRange": {
				"start": { "line": 0, "character": 0 },
				"end": { "line": 0, "character": 5 },
			},
			"targetSelectionRange": {
				"start": { "line": 0, "character": 0 },
				"end": { "line": 0, "character": 5 },
			},
		}]);
		enrich_locations(&mut result);
		// Non-file URI should not produce a context_line.
		assert!(result[0].get("context_line").is_none());
	}

	#[test]
	fn enrich_locations_location_link_missing_file() {
		let mut result = json!([{
			"targetUri": "file:///nonexistent/path.rs",
			"targetRange": {
				"start": { "line": 0, "character": 0 },
				"end": { "line": 0, "character": 5 },
			},
		}]);
		enrich_locations(&mut result);
		assert!(result[0].get("context_line").is_none());
	}

	#[test]
	fn enrich_locations_object_without_uri_or_target_uri() {
		let mut result = json!({ "something": "else" });
		enrich_locations(&mut result);
		// Should not crash or add context_line.
		assert!(result.get("context_line").is_none());
	}

	#[test]
	fn enrich_locations_null_result() {
		let mut result = Value::Null;
		enrich_locations(&mut result);
		assert_eq!(result, Value::Null);
	}

	#[test]
	fn enrich_locations_string_result() {
		let mut result = json!("not a location");
		enrich_locations(&mut result);
		assert_eq!(result, json!("not a location"));
	}

	#[test]
	fn enrich_locations_empty_array() {
		let mut result = json!([]);
		enrich_locations(&mut result);
		assert_eq!(result, json!([]));
	}

	// -- format_do_output ---------------------------------------------------

	#[test]
	fn format_do_output_json_hover() {
		let dir = unique_temp_dir("fdo_json");
		let file = write_temp_file(&dir, "t.rs", "");
		let pos = make_position_args(&file);
		let method = DoMethod::Hover(pos);
		let (_, meta) = build_request(&method).unwrap();

		let call_ok = CallOk {
			lease_id: "lease-1".to_string(),
			response: json!({
				"jsonrpc": "2.0",
				"id": 1,
				"result": { "contents": "fn main()" }
			}),
		};

		let output = format_do_output(
			&method,
			"rust-analyzer",
			&meta,
			&call_ok,
			42,
			DoOutput::Json,
		)
		.unwrap();

		let parsed: Value = serde_json::from_str(&output).unwrap();
		assert_eq!(parsed["lsp_id"], "rust-analyzer");
		assert_eq!(parsed["method"], "textDocument/hover");
		assert_eq!(parsed["elapsed_ms"], 42);
		assert_eq!(parsed["result"]["contents"], "fn main()");
		// Compact JSON = single line.
		assert!(!output.contains('\n'));
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn format_do_output_pretty() {
		let dir = unique_temp_dir("fdo_pretty");
		let file = write_temp_file(&dir, "t.rs", "");
		let pos = make_position_args(&file);
		let method = DoMethod::Hover(pos);
		let (_, meta) = build_request(&method).unwrap();

		let call_ok = CallOk {
			lease_id: "lease-1".to_string(),
			response: json!({ "jsonrpc": "2.0", "id": 1, "result": null }),
		};

		let output =
			format_do_output(&method, "test", &meta, &call_ok, 0, DoOutput::Pretty).unwrap();

		// Pretty JSON = contains newlines and indentation.
		assert!(output.contains('\n'));
		assert!(output.contains("  "));
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn format_do_output_enriches_definition_locations() {
		let dir = unique_temp_dir("fdo_enrich");
		let source = write_temp_file(&dir, "lib.rs", "fn hello() {}\nfn world() {}\n");
		let uri = file_uri(&source).unwrap();

		let pos = make_position_args(&source);
		let method = DoMethod::Definition(pos);
		let (_, meta) = build_request(&method).unwrap();

		let call_ok = CallOk {
			lease_id: "lease-1".to_string(),
			response: json!({
				"jsonrpc": "2.0",
				"id": 1,
				"result": [{
					"uri": uri,
					"range": {
						"start": { "line": 1, "character": 0 },
						"end": { "line": 1, "character": 14 },
					}
				}]
			}),
		};

		let output = format_do_output(&method, "ra", &meta, &call_ok, 5, DoOutput::Json).unwrap();

		let parsed: Value = serde_json::from_str(&output).unwrap();
		assert_eq!(parsed["result"][0]["context_line"], "fn world() {}");
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn format_do_output_no_enrichment_for_hover() {
		let dir = unique_temp_dir("fdo_no_enrich");
		let file = write_temp_file(&dir, "t.rs", "");
		let pos = make_position_args(&file);
		let method = DoMethod::Hover(pos);
		let (_, meta) = build_request(&method).unwrap();

		let call_ok = CallOk {
			lease_id: "lease-1".to_string(),
			response: json!({
				"jsonrpc": "2.0",
				"id": 1,
				"result": { "contents": "info" }
			}),
		};

		let output = format_do_output(&method, "ra", &meta, &call_ok, 1, DoOutput::Json).unwrap();

		let parsed: Value = serde_json::from_str(&output).unwrap();
		// Hover results should not have context_line.
		assert!(parsed["result"].get("context_line").is_none());
		fs::remove_dir_all(&dir).ok();
	}

	#[test]
	fn format_do_output_with_lsp_error() {
		let method = DoMethod::WorkspaceSymbols(WorkspaceSymbolArgs {
			lsp: "test".to_string(),
			root: None,
			no_start_daemon: false,
			output: DoOutput::Json,
			query: "q".to_string(),
		});
		let (_, meta) = build_request(&method).unwrap();

		let call_ok = CallOk {
			lease_id: "lease-1".to_string(),
			response: json!({
				"jsonrpc": "2.0",
				"id": 1,
				"error": { "code": -32601, "message": "not supported" }
			}),
		};

		let output =
			format_do_output(&method, "test", &meta, &call_ok, 10, DoOutput::Json).unwrap();

		let parsed: Value = serde_json::from_str(&output).unwrap();
		assert_eq!(parsed["result"]["error"]["code"], -32601);
	}

	// -- language_id_for_extension -------------------------------------------

	#[test]
	fn language_id_rust() {
		assert_eq!(language_id_for_extension("rs"), "rust");
	}

	#[test]
	fn language_id_typescript() {
		assert_eq!(language_id_for_extension("ts"), "typescript");
	}

	#[test]
	fn language_id_typescriptreact() {
		assert_eq!(language_id_for_extension("tsx"), "typescriptreact");
	}

	#[test]
	fn language_id_javascript() {
		assert_eq!(language_id_for_extension("js"), "javascript");
	}

	#[test]
	fn language_id_javascriptreact() {
		assert_eq!(language_id_for_extension("jsx"), "javascriptreact");
	}

	#[test]
	fn language_id_python() {
		assert_eq!(language_id_for_extension("py"), "python");
	}

	#[test]
	fn language_id_go() {
		assert_eq!(language_id_for_extension("go"), "go");
	}

	#[test]
	fn language_id_toml() {
		assert_eq!(language_id_for_extension("toml"), "toml");
	}

	#[test]
	fn language_id_json() {
		assert_eq!(language_id_for_extension("json"), "json");
	}

	#[test]
	fn language_id_markdown() {
		assert_eq!(language_id_for_extension("md"), "markdown");
	}

	#[test]
	fn language_id_yaml() {
		assert_eq!(language_id_for_extension("yaml"), "yaml");
	}

	#[test]
	fn language_id_yml() {
		assert_eq!(language_id_for_extension("yml"), "yaml");
	}

	#[test]
	fn language_id_html() {
		assert_eq!(language_id_for_extension("html"), "html");
	}

	#[test]
	fn language_id_css() {
		assert_eq!(language_id_for_extension("css"), "css");
	}

	#[test]
	fn language_id_unknown_defaults_to_plaintext() {
		assert_eq!(language_id_for_extension("xyz"), "plaintext");
		assert_eq!(language_id_for_extension(""), "plaintext");
	}

	// -- language_id_for_path -----------------------------------------------

	#[test]
	fn language_id_for_path_with_extension() {
		assert_eq!(language_id_for_path(Path::new("src/main.rs")), "rust");
		assert_eq!(
			language_id_for_path(Path::new("/home/user/app.tsx")),
			"typescriptreact"
		);
	}

	#[test]
	fn language_id_for_path_no_extension() {
		assert_eq!(language_id_for_path(Path::new("Makefile")), "plaintext");
	}

	// -- integration: run_async with live daemon ----------------------------

	/// Create a short temp dir under `/tmp` for daemon tests. Unix socket
	/// paths have a ~108 byte limit on macOS, so the standard temp dir
	/// (which can be very long) causes `bind` to fail.
	fn short_temp_dir(name: &str) -> PathBuf {
		let nanos = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("time")
			.as_nanos();
		let dir = PathBuf::from("/tmp").join(format!("lspee-do-{name}-{nanos}"));
		fs::create_dir_all(&dir).expect("should create dir");
		fs::canonicalize(&dir).expect("should canonicalize")
	}

	/// Write a project config that uses `cat` as a mock LSP server.
	fn write_cat_project_config(root: &Path) {
		let config = "workspace_mode = \"single\"\n\n[[lsp]]\nid = \"cat-lsp\"\ncommand = \
		              \"cat\"\nargs = []\n";
		fs::write(root.join("lspee.toml"), config).expect("write config");
	}

	/// Spawn a daemon in a background tokio task.
	fn spawn_test_daemon(root: &Path) -> tokio::task::JoinHandle<Result<()>> {
		let resolved = lspee_config::resolve(Some(root)).expect("config should resolve");
		let daemon = lspee_daemon::Daemon::new(root.to_path_buf(), resolved);
		tokio::spawn(async move { daemon.run().await })
	}

	/// Wait for the daemon socket to become connectable.
	async fn wait_for_daemon(root: &Path) {
		let socket = root.join(".lspee").join("daemon.sock");
		for _ in 0..100 {
			if tokio::net::UnixStream::connect(&socket).await.is_ok() {
				return;
			}
			tokio::time::sleep(std::time::Duration::from_millis(25)).await;
		}
		panic!(
			"daemon socket never became available at {}",
			socket.display()
		);
	}

	/// Shutdown daemon gracefully.
	async fn shutdown_test_daemon(root: &Path) {
		let socket = root.join(".lspee").join("daemon.sock");
		if let Ok(stream) = tokio::net::UnixStream::connect(&socket).await {
			let (reader, mut writer) = stream.into_split();
			let mut lines = tokio::io::BufReader::new(reader).lines();
			let id = client::new_request_id("shutdown");
			let env = ControlEnvelope {
				v: lspee_daemon::PROTOCOL_VERSION,
				id: Some(id.clone()),
				message_type: "Shutdown".to_string(),
				payload: serde_json::to_value(lspee_daemon::Shutdown {}).unwrap(),
			};
			let _ = client::write_frame(&mut writer, &env).await;
			let _ = client::read_response_for_id(&mut lines, &id).await;
		}
	}

	#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
	async fn run_async_hover_json_with_live_daemon() {
		let dir = short_temp_dir("hover");
		write_cat_project_config(&dir);
		let source = write_temp_file(&dir, "main.rs", "fn main() {}\n");

		let _daemon = spawn_test_daemon(&dir);
		wait_for_daemon(&dir).await;

		let cmd = DoCommand {
			method: DoMethod::Hover(PositionArgs {
				shared: SharedArgs {
					lsp: Some("cat-lsp".to_string()),
					root: Some(dir.clone()),
					no_start_daemon: true,
					output: DoOutput::Json,
				},
				file: source,
				line: 0,
				col: 0,
			}),
		};

		let result = run_async(cmd).await;
		assert!(result.is_ok(), "run_async hover json failed: {result:?}");

		shutdown_test_daemon(&dir).await;
		fs::remove_dir_all(&dir).ok();
	}

	#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
	async fn run_async_hover_pretty_with_live_daemon() {
		let dir = short_temp_dir("pretty");
		write_cat_project_config(&dir);
		let source = write_temp_file(&dir, "lib.rs", "struct Foo;\n");

		let _daemon = spawn_test_daemon(&dir);
		wait_for_daemon(&dir).await;

		let cmd = DoCommand {
			method: DoMethod::Hover(PositionArgs {
				shared: SharedArgs {
					lsp: Some("cat-lsp".to_string()),
					root: Some(dir.clone()),
					no_start_daemon: true,
					output: DoOutput::Pretty,
				},
				file: source,
				line: 0,
				col: 0,
			}),
		};

		let result = run_async(cmd).await;
		assert!(result.is_ok(), "run_async hover pretty failed: {result:?}");

		shutdown_test_daemon(&dir).await;
		fs::remove_dir_all(&dir).ok();
	}

	#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
	async fn run_async_definition_with_live_daemon() {
		let dir = short_temp_dir("defn");
		write_cat_project_config(&dir);
		let source = write_temp_file(&dir, "src.rs", "pub fn greet() {}\n");

		let _daemon = spawn_test_daemon(&dir);
		wait_for_daemon(&dir).await;

		let cmd = DoCommand {
			method: DoMethod::Definition(PositionArgs {
				shared: SharedArgs {
					lsp: Some("cat-lsp".to_string()),
					root: Some(dir.clone()),
					no_start_daemon: true,
					output: DoOutput::Json,
				},
				file: source,
				line: 0,
				col: 0,
			}),
		};

		let result = run_async(cmd).await;
		assert!(result.is_ok(), "run_async definition failed: {result:?}");

		shutdown_test_daemon(&dir).await;
		fs::remove_dir_all(&dir).ok();
	}
}
