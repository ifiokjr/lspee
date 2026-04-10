pub mod languages;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
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
pub struct SessionConfig {
    #[serde(default = "default_idle_ttl_secs")]
    pub idle_ttl_secs: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            idle_ttl_secs: default_idle_ttl_secs(),
        }
    }
}

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
    #[serde(default, deserialize_with = "deserialize_lsp_entries")]
    pub lsp: Vec<PartialLspConfig>,
    pub root_markers: Option<Vec<String>>,
    pub workspace_mode: Option<String>,
    pub transport_flags: Option<BTreeMap<String, String>>,
    pub memory: Option<PartialMemoryConfig>,
    pub session: Option<PartialSessionConfig>,
}

fn deserialize_lsp_entries<'de, D>(deserializer: D) -> Result<Vec<PartialLspConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum LspField {
        Single(PartialLspConfig),
        Multiple(Vec<PartialLspConfig>),
    }

    Ok(match LspField::deserialize(deserializer)? {
        LspField::Single(single) => vec![single],
        LspField::Multiple(multiple) => multiple,
    })
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
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"));
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
    }
}
