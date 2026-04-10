use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::{ConfigError, PartialConfig};

const DEFAULT_LANGUAGES_TOML: &str = include_str!("../defaults/languages.toml");

#[derive(Debug, Clone, Serialize)]
pub struct LspSelection {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub root_markers: Vec<String>,
    pub executable_found: bool,
}

#[derive(Debug, Default, Deserialize)]
struct LanguageRegistryFile {
    #[serde(default)]
    lsp: BTreeMap<String, LspDefinition>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct LspDefinition {
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    file_extensions: Vec<String>,
    #[serde(default)]
    root_markers: Vec<String>,
}

pub fn lsp_for_id(
    lsp_id: &str,
    user_config: Option<&Path>,
    project_config: Option<&Path>,
) -> Result<Option<LspSelection>, ConfigError> {
    let registry = load_registry(user_config, project_config)?;

    Ok(registry
        .lsp
        .get(lsp_id)
        .map(|definition| selection_from_definition(lsp_id.to_string(), definition.clone())))
}

pub fn lsps_for_file(
    file_path: &Path,
    user_config: Option<&Path>,
    project_config: Option<&Path>,
) -> Result<Vec<LspSelection>, ConfigError> {
    let extension = file_path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase);

    let Some(extension) = extension else {
        return Ok(Vec::new());
    };

    let registry = load_registry(user_config, project_config)?;
    let mut matches = Vec::new();

    for (id, definition) in registry.lsp {
        if definition
            .file_extensions
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&extension))
        {
            matches.push(selection_from_definition(id, definition));
        }
    }

    Ok(matches)
}

fn load_registry(
    user_config: Option<&Path>,
    project_config: Option<&Path>,
) -> Result<LanguageRegistryFile, ConfigError> {
    let mut registry: LanguageRegistryFile =
        toml::from_str(DEFAULT_LANGUAGES_TOML).map_err(|source| ConfigError::Parse {
            path: Path::new("crates/lspee_config/defaults/languages.toml").to_path_buf(),
            source,
        })?;

    apply_overrides(&mut registry, user_config)?;
    apply_overrides(&mut registry, project_config)?;

    Ok(registry)
}

fn selection_from_definition(id: String, definition: LspDefinition) -> LspSelection {
    LspSelection {
        executable_found: has_executable(&definition.command),
        id,
        command: definition.command,
        args: definition.args,
        root_markers: definition.root_markers,
    }
}

fn apply_overrides(
    registry: &mut LanguageRegistryFile,
    path: Option<&Path>,
) -> Result<(), ConfigError> {
    let Some(path) = path else {
        return Ok(());
    };

    if !path.exists() {
        return Ok(());
    }

    let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let parsed: PartialConfig = toml::from_str(&raw).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })?;

    for partial_lsp in parsed.lsp {
        let id = partial_lsp.id.unwrap_or_else(|| "default".to_string());
        let entry = registry.lsp.entry(id).or_default();

        if let Some(command) = partial_lsp.command {
            entry.command = command;
        }

        if let Some(args) = partial_lsp.args {
            entry.args = args;
        }

        if let Some(init_options) = partial_lsp.initialization_options {
            let mut extension_set: BTreeSet<String> =
                entry.file_extensions.iter().cloned().collect();
            for key in init_options.keys() {
                if let Some(ext) = key.strip_prefix("language_extension_") {
                    extension_set.insert(ext.to_string());
                }
            }
            entry.file_extensions = extension_set.into_iter().collect();
        }
    }

    Ok(())
}

fn has_executable(executable: &str) -> bool {
    if executable.is_empty() {
        return false;
    }

    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let candidate = dir.join(executable);
            if candidate.is_file() {
                return true;
            }

            #[cfg(windows)]
            {
                let candidate_exe = dir.join(format!("{executable}.exe"));
                candidate_exe.is_file()
            }

            #[cfg(not(windows))]
            {
                false
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_LANGUAGES_TOML, LanguageRegistryFile, lsp_for_id, lsps_for_file};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_temp_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        std::env::temp_dir().join(format!("lspee-{name}-{nanos}.toml"))
    }

    #[test]
    fn default_registry_contains_at_least_100_lsps() {
        let parsed: LanguageRegistryFile =
            toml::from_str(DEFAULT_LANGUAGES_TOML).expect("default languages.toml should parse");

        assert!(
            parsed.lsp.len() >= 100,
            "expected at least 100 default lsp definitions, found {}",
            parsed.lsp.len()
        );
    }

    #[test]
    fn rust_file_maps_to_rust_analyzer() {
        let matches = lsps_for_file(Path::new("src/main.rs"), None, None)
            .expect("default mapping should resolve");

        assert!(
            matches
                .iter()
                .any(|selection| selection.id == "rust-analyzer"),
            "expected rust-analyzer mapping for .rs files"
        );
    }

    #[test]
    fn lsp_for_id_returns_command_from_default_registry() {
        let selection = lsp_for_id("rust-analyzer", None, None)
            .expect("default lookup should work")
            .expect("rust-analyzer should exist");

        assert_eq!(selection.command, "rust-analyzer");
    }

    #[test]
    fn project_override_can_change_command_and_add_extension() {
        let project_config = unique_temp_file("override");
        let config = r#"
[[lsp]]
id = "rust-analyzer"
command = "custom-ra"
args = ["--stdio"]
initialization_options = { language_extension_foo = true }
"#;

        fs::write(&project_config, config).expect("should write temp project config");

        let matches = lsps_for_file(Path::new("lib.foo"), None, Some(&project_config))
            .expect("override mapping should resolve");

        let rust_selection = matches
            .iter()
            .find(|selection| selection.id == "rust-analyzer")
            .expect("rust-analyzer should be selected for .foo after override");

        assert_eq!(rust_selection.command, "custom-ra");
        assert_eq!(rust_selection.args, vec!["--stdio".to_string()]);

        let _ = fs::remove_file(&project_config);
    }
}
