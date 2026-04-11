pub mod languages;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use thiserror::Error;

const PROJECT_CONFIG_FILE: &str = "lspee.toml";
const USER_CONFIG_DIR: &str = ".config/lspee";
const USER_CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Error)]
pub enum ConfigError {
	#[error("failed to read {path}: {source}")]
	Read {
		path: PathBuf,
		source: std::io::Error,
	},
	#[error("failed to parse TOML {path}: {source}")]
	Parse {
		path: PathBuf,
		source: toml::de::Error,
	},
	#[error("failed to canonicalize root {path}: {source}")]
	Canonicalize {
		path: PathBuf,
		source: std::io::Error,
	},
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EffectiveConfig {
	#[serde(default)]
	pub lsps: BTreeMap<String, LspConfig>,
	#[serde(default)]
	pub root_markers: Vec<String>,
	#[serde(default)]
	pub workspace_mode: String,
	#[serde(default)]
	pub transport_flags: BTreeMap<String, String>,
	#[serde(default)]
	pub memory: MemoryConfig,
	#[serde(default)]
	pub session: SessionConfig,
}

impl EffectiveConfig {
	/// Look up the config for a specific LSP by its id.
	///
	/// # Examples
	///
	/// ```
	/// use lspee_config::{EffectiveConfig, LspConfig};
	///
	/// let mut config = EffectiveConfig::default();
	/// config.lsps.insert(
	/// 	"rust-analyzer".to_string(),
	/// 	LspConfig {
	/// 		id: "rust-analyzer".to_string(),
	/// 		command: "rust-analyzer".to_string(),
	/// 		..Default::default()
	/// 	},
	/// );
	/// assert!(config.lsp_config("rust-analyzer").is_some());
	/// assert!(config.lsp_config("nonexistent").is_none());
	/// ```
	pub fn lsp_config(&self, id: &str) -> Option<&LspConfig> {
		self.lsps.get(id)
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
	#[serde(default)]
	pub max_session_mb: Option<u64>,
	#[serde(default)]
	pub max_total_mb: Option<u64>,
	#[serde(default = "default_memory_check_interval_ms")]
	pub check_interval_ms: u64,
}

impl Default for MemoryConfig {
	fn default() -> Self {
		Self {
			max_session_mb: None,
			max_total_mb: None,
			check_interval_ms: default_memory_check_interval_ms(),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
	pub idle_ttl_secs: u64,
	/// How long the daemon stays alive with zero sessions before
	/// shutting itself down. `None` means the daemon runs forever.
	/// Default: 1800 (30 minutes).
	pub daemon_idle_ttl_secs: Option<u64>,
}

impl Default for SessionConfig {
	fn default() -> Self {
		Self {
			idle_ttl_secs: default_idle_ttl_secs(),
			daemon_idle_ttl_secs: Some(1800),
		}
	}
}

/// Configuration for a single LSP server.
///
/// # Examples
///
/// ```
/// use lspee_config::LspConfig;
///
/// let config = LspConfig {
/// 	id: "rust-analyzer".to_string(),
/// 	command: "rust-analyzer".to_string(),
/// 	args: vec!["--stdio".to_string()],
/// 	..Default::default()
/// };
/// assert_eq!(config.command, "rust-analyzer");
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LspConfig {
	#[serde(default)]
	pub id: String,
	#[serde(default)]
	pub command: String,
	#[serde(default)]
	pub args: Vec<String>,
	#[serde(default)]
	pub env: BTreeMap<String, String>,
	#[serde(default)]
	pub initialization_options: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PartialConfig {
	#[serde(default)]
	pub lsp: Vec<PartialLspConfig>,
	pub root_markers: Option<Vec<String>>,
	pub workspace_mode: Option<String>,
	pub transport_flags: Option<BTreeMap<String, String>>,
	pub memory: Option<PartialMemoryConfig>,
	pub session: Option<PartialSessionConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartialMemoryConfig {
	pub max_session_mb: Option<u64>,
	pub max_total_mb: Option<u64>,
	pub check_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartialSessionConfig {
	pub idle_ttl_secs: Option<u64>,
	pub daemon_idle_ttl_secs: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartialLspConfig {
	pub id: Option<String>,
	pub command: Option<String>,
	pub args: Option<Vec<String>>,
	pub env: Option<BTreeMap<String, String>>,
	pub initialization_options: Option<BTreeMap<String, toml::Value>>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
	pub project_root: PathBuf,
	pub merged: EffectiveConfig,
	pub config_hash: String,
}

pub fn resolve(project_root_override: Option<&Path>) -> Result<ResolvedConfig, ConfigError> {
	let project_root = canonical_project_root(project_root_override)?;

	let mut merged = default_config();
	let user_path = user_config_path();
	if let Some(user_cfg) = load_partial_if_exists(&user_path)? {
		apply_partial(&mut merged, user_cfg);
	}

	let project_path = project_root.join(PROJECT_CONFIG_FILE);
	if let Some(project_cfg) = load_partial_if_exists(&project_path)? {
		apply_partial(&mut merged, project_cfg);
	}

	let config_hash = hash_identity(&project_root, &merged);

	Ok(ResolvedConfig {
		project_root,
		merged,
		config_hash,
	})
}

/// Compute a deterministic SHA-256 hash that identifies the combination of
/// a project root and its effective configuration.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use lspee_config::{EffectiveConfig, hash_identity};
///
/// let root = Path::new("/tmp/my-project");
/// let config = EffectiveConfig::default();
/// let hash = hash_identity(root, &config);
/// assert_eq!(hash.len(), 64); // SHA-256 hex string
/// assert_eq!(hash, hash_identity(root, &config)); // deterministic
/// ```
pub fn hash_identity(project_root: &Path, merged_config: &EffectiveConfig) -> String {
	let mut hasher = Sha256::new();
	hasher.update(project_root.to_string_lossy().as_bytes());
	hasher.update([0]);
	let canonical = toml::to_string(merged_config).expect("effective config is serializable");
	hasher.update(canonical.as_bytes());
	format!("{:x}", hasher.finalize())
}

fn canonical_project_root(project_root_override: Option<&Path>) -> Result<PathBuf, ConfigError> {
	let root = match project_root_override {
		Some(path) => path.to_path_buf(),
		None => std::env::current_dir().map_err(|source| ConfigError::Read {
			path: PathBuf::from("."),
			source,
		})?,
	};

	fs::canonicalize(&root).map_err(|source| ConfigError::Canonicalize { path: root, source })
}

fn load_partial_if_exists(path: &Path) -> Result<Option<PartialConfig>, ConfigError> {
	if !path.exists() {
		return Ok(None);
	}

	let raw = fs::read_to_string(path).map_err(|source| ConfigError::Read {
		path: path.to_path_buf(),
		source,
	})?;

	let parsed = toml::from_str(&raw).map_err(|source| ConfigError::Parse {
		path: path.to_path_buf(),
		source,
	})?;

	Ok(Some(parsed))
}

fn user_config_path() -> PathBuf {
	let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("~"), PathBuf::from);
	home.join(USER_CONFIG_DIR).join(USER_CONFIG_FILE)
}

fn default_memory_check_interval_ms() -> u64 {
	1_000
}

fn default_idle_ttl_secs() -> u64 {
	300
}

fn default_config() -> EffectiveConfig {
	EffectiveConfig {
		lsps: BTreeMap::new(),
		root_markers: vec![".git".to_string()],
		workspace_mode: "single".to_string(),
		transport_flags: BTreeMap::new(),
		memory: MemoryConfig::default(),
		session: SessionConfig::default(),
	}
}

fn apply_partial(merged: &mut EffectiveConfig, partial: PartialConfig) {
	for lsp in partial.lsp {
		let id = lsp.id.clone().unwrap_or_else(|| "default".to_string());
		let entry = merged.lsps.entry(id.clone()).or_insert_with(|| LspConfig {
			id: id.clone(),
			command: String::new(),
			args: Vec::new(),
			env: BTreeMap::new(),
			initialization_options: BTreeMap::new(),
		});

		if let Some(command) = lsp.command {
			entry.command = command;
		}
		if let Some(args) = lsp.args {
			entry.args = args;
		}
		if let Some(env) = lsp.env {
			entry.env = env;
		}
		if let Some(initialization_options) = lsp.initialization_options {
			entry.initialization_options = initialization_options;
		}
	}

	if let Some(root_markers) = partial.root_markers {
		merged.root_markers = root_markers;
	}
	if let Some(workspace_mode) = partial.workspace_mode {
		merged.workspace_mode = workspace_mode;
	}
	if let Some(transport_flags) = partial.transport_flags {
		merged.transport_flags = transport_flags;
	}
	if let Some(memory) = partial.memory {
		if let Some(max_session_mb) = memory.max_session_mb {
			merged.memory.max_session_mb = Some(max_session_mb);
		}
		if let Some(max_total_mb) = memory.max_total_mb {
			merged.memory.max_total_mb = Some(max_total_mb);
		}
		if let Some(check_interval_ms) = memory.check_interval_ms {
			merged.memory.check_interval_ms = check_interval_ms;
		}
	}
	if let Some(session) = partial.session {
		if let Some(idle_ttl_secs) = session.idle_ttl_secs {
			merged.session.idle_ttl_secs = idle_ttl_secs;
		}
		if let Some(daemon_idle_ttl_secs) = session.daemon_idle_ttl_secs {
			merged.session.daemon_idle_ttl_secs = Some(daemon_idle_ttl_secs);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_multiple_lsp_array_format() {
		let toml_str = r#"
[[lsp]]
id = "rust-analyzer"
command = "rust-analyzer"
args = []

[[lsp]]
id = "taplo"
command = "taplo"
args = ["lsp", "stdio"]
"#;
		let partial: PartialConfig = toml::from_str(toml_str).expect("should parse [[lsp]] array");
		assert_eq!(partial.lsp.len(), 2);
		assert_eq!(partial.lsp[0].id.as_deref(), Some("rust-analyzer"));
		assert_eq!(partial.lsp[1].id.as_deref(), Some("taplo"));
		assert_eq!(
			partial.lsp[1].args.as_deref(),
			Some(vec!["lsp".to_string(), "stdio".to_string()].as_slice())
		);
	}

	#[test]
	fn parse_config_without_lsp_section() {
		let toml_str = r#"
workspace_mode = "single"

[session]
idle_ttl_secs = 600
"#;
		let partial: PartialConfig = toml::from_str(toml_str).expect("should parse without lsp");
		assert!(partial.lsp.is_empty());
		assert_eq!(partial.session.unwrap().idle_ttl_secs, Some(600));
	}

	#[test]
	fn apply_partial_merges_multiple_lsps_by_id() {
		let mut merged = default_config();

		let partial = PartialConfig {
			lsp: vec![
				PartialLspConfig {
					id: Some("rust-analyzer".to_string()),
					command: Some("rust-analyzer".to_string()),
					args: None,
					env: None,
					initialization_options: None,
				},
				PartialLspConfig {
					id: Some("taplo".to_string()),
					command: Some("taplo".to_string()),
					args: Some(vec!["lsp".to_string(), "stdio".to_string()]),
					env: None,
					initialization_options: None,
				},
			],
			root_markers: None,
			workspace_mode: None,
			transport_flags: None,
			memory: None,
			session: None,
		};

		apply_partial(&mut merged, partial);

		assert_eq!(merged.lsps.len(), 2);
		assert_eq!(merged.lsps["rust-analyzer"].command, "rust-analyzer");
		assert_eq!(merged.lsps["taplo"].command, "taplo");
		assert_eq!(
			merged.lsps["taplo"].args,
			vec!["lsp".to_string(), "stdio".to_string()]
		);
	}

	#[test]
	fn apply_partial_overwrites_same_id() {
		let mut merged = default_config();

		let first = PartialConfig {
			lsp: vec![PartialLspConfig {
				id: Some("ra".to_string()),
				command: Some("old-command".to_string()),
				args: Some(vec!["--old".to_string()]),
				env: None,
				initialization_options: None,
			}],
			..Default::default()
		};
		apply_partial(&mut merged, first);
		assert_eq!(merged.lsps["ra"].command, "old-command");

		let second = PartialConfig {
			lsp: vec![PartialLspConfig {
				id: Some("ra".to_string()),
				command: Some("new-command".to_string()),
				args: None,
				env: None,
				initialization_options: None,
			}],
			..Default::default()
		};
		apply_partial(&mut merged, second);
		assert_eq!(merged.lsps["ra"].command, "new-command");
		// args from first layer are preserved since second didn't set them
		assert_eq!(merged.lsps["ra"].args, vec!["--old".to_string()]);
	}

	#[test]
	fn lsp_config_lookup_by_id() {
		let mut config = default_config();
		config.lsps.insert(
			"rust-analyzer".to_string(),
			LspConfig {
				id: "rust-analyzer".to_string(),
				command: "rust-analyzer".to_string(),
				..Default::default()
			},
		);

		assert!(config.lsp_config("rust-analyzer").is_some());
		assert!(config.lsp_config("nonexistent").is_none());
	}

	#[test]
	fn default_config_has_empty_lsps() {
		let config = default_config();
		assert!(config.lsps.is_empty());
		assert_eq!(config.root_markers, vec![".git".to_string()]);
		assert_eq!(config.session.idle_ttl_secs, 300);
	}

	#[test]
	fn config_hash_is_deterministic() {
		let root = PathBuf::from("/tmp/test-hash-determinism");
		let config = default_config();
		let hash1 = hash_identity(&root, &config);
		let hash2 = hash_identity(&root, &config);
		assert_eq!(hash1, hash2);
	}

	#[test]
	fn config_hash_changes_with_lsp_entries() {
		let root = PathBuf::from("/tmp/test-hash-change");
		let config1 = default_config();
		let mut config2 = default_config();
		config2.lsps.insert(
			"ra".to_string(),
			LspConfig {
				id: "ra".to_string(),
				command: "rust-analyzer".to_string(),
				..Default::default()
			},
		);
		assert_ne!(
			hash_identity(&root, &config1),
			hash_identity(&root, &config2)
		);
	}

	#[test]
	fn resolve_from_temp_dir_with_multi_lsp_config() {
		let dir = std::env::temp_dir().join(format!(
			"lspee-test-resolve-{}",
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_nanos()
		));
		fs::create_dir_all(&dir).unwrap();

		let config = r#"
[[lsp]]
id = "rust-analyzer"
command = "rust-analyzer"

[[lsp]]
id = "taplo"
command = "taplo"
args = ["lsp", "stdio"]
"#;
		fs::write(dir.join("lspee.toml"), config).unwrap();

		let resolved = resolve(Some(&dir)).expect("should resolve");
		assert_eq!(resolved.merged.lsps.len(), 2);
		assert!(resolved.merged.lsp_config("rust-analyzer").is_some());
		assert!(resolved.merged.lsp_config("taplo").is_some());
		assert_eq!(
			resolved.merged.lsp_config("taplo").unwrap().args,
			vec!["lsp".to_string(), "stdio".to_string()]
		);

		let _ = fs::remove_dir_all(&dir);
	}
}
