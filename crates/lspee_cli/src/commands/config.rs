use clap::{Args, Subcommand, ValueEnum};
use lspee_config::resolve;
use std::path::{Path, PathBuf};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value as TomlValue};

#[derive(Debug, Args)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Show the resolved effective configuration.
    Show(ShowCommand),
    /// Create an initial lspee.toml in the project root.
    Init(InitCommand),
    /// Add or update an LSP definition in the project config.
    AddLsp(AddLspCommand),
    /// Remove an LSP definition from the project config.
    RemoveLsp(RemoveLspCommand),
    /// Set a scalar configuration value.
    Set(SetCommand),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ConfigOutput {
    Human,
    Json,
}

#[derive(Debug, Args)]
pub struct ShowCommand {
    /// Override project root.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = ConfigOutput::Json)]
    pub output: ConfigOutput,
}

#[derive(Debug, Args)]
pub struct InitCommand {
    /// Override project root.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Overwrite existing lspee.toml.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct AddLspCommand {
    /// LSP identifier (e.g. rust-analyzer, taplo).
    #[arg(long)]
    pub id: String,

    /// Command to run the LSP server.
    #[arg(long)]
    pub command: String,

    /// Arguments to pass to the LSP command.
    #[arg(long, value_delimiter = ' ')]
    pub args: Option<Vec<String>>,

    /// Override project root.
    #[arg(long)]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct RemoveLspCommand {
    /// LSP identifier to remove.
    #[arg(long)]
    pub id: String,

    /// Override project root.
    #[arg(long)]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct SetCommand {
    /// Config key in dotted notation (e.g. session.idle_ttl_secs, memory.max_total_mb).
    pub key: String,

    /// Value to set.
    pub value: String,

    /// Override project root.
    #[arg(long)]
    pub root: Option<PathBuf>,
}

pub fn run(cmd: ConfigCommand) -> anyhow::Result<()> {
    match cmd.action {
        ConfigAction::Show(cmd) => run_show(cmd),
        ConfigAction::Init(cmd) => run_init(cmd),
        ConfigAction::AddLsp(cmd) => run_add_lsp(cmd),
        ConfigAction::RemoveLsp(cmd) => run_remove_lsp(cmd),
        ConfigAction::Set(cmd) => run_set(cmd),
    }
}

fn run_show(cmd: ShowCommand) -> anyhow::Result<()> {
    let resolved = resolve(cmd.root.as_deref())?;

    match cmd.output {
        ConfigOutput::Human => {
            println!("project_root={}", resolved.project_root.display());
            println!("config_hash={}", resolved.config_hash);
            println!();
            let toml_str = toml::to_string_pretty(&resolved.merged)?;
            print!("{toml_str}");
        }
        ConfigOutput::Json => {
            let payload = serde_json::json!({
                "project_root": resolved.project_root,
                "config_hash": resolved.config_hash,
                "config": resolved.merged,
            });
            println!("{}", serde_json::to_string(&payload)?);
        }
    }

    Ok(())
}

fn run_init(cmd: InitCommand) -> anyhow::Result<()> {
    let project_root = canonical_root(cmd.root.as_deref())?;
    let config_path = project_root.join("lspee.toml");

    if config_path.exists() && !cmd.force {
        anyhow::bail!(
            "lspee.toml already exists at {}. Use --force to overwrite.",
            config_path.display()
        );
    }

    let template = generate_init_template(&project_root);
    std::fs::write(&config_path, &template)?;
    println!("{}", config_path.display());
    Ok(())
}

fn run_add_lsp(cmd: AddLspCommand) -> anyhow::Result<()> {
    let project_root = canonical_root(cmd.root.as_deref())?;
    let config_path = project_root.join("lspee.toml");
    let mut doc = load_or_create_doc(&config_path)?;

    let lsp_array = ensure_lsp_array(&mut doc);

    // Check if entry with this id already exists; update it if so.
    let mut found = false;
    for entry in lsp_array.iter_mut() {
        if entry.get("id").and_then(|v| v.as_str()) == Some(&cmd.id) {
            entry["command"] = toml_edit::value(&cmd.command);
            if let Some(ref args) = cmd.args {
                entry["args"] = toml_edit::value(to_toml_array(args));
            }
            found = true;
            break;
        }
    }

    if !found {
        let mut table = Table::new();
        table.insert("id", toml_edit::value(&cmd.id));
        table.insert("command", toml_edit::value(&cmd.command));
        let args = cmd.args.as_deref().unwrap_or(&[]);
        table.insert("args", toml_edit::value(to_toml_array(args)));
        lsp_array.push(table);
    }

    std::fs::write(&config_path, doc.to_string())?;
    println!("{}", if found { "updated" } else { "added" });
    Ok(())
}

fn run_remove_lsp(cmd: RemoveLspCommand) -> anyhow::Result<()> {
    let project_root = canonical_root(cmd.root.as_deref())?;
    let config_path = project_root.join("lspee.toml");

    if !config_path.exists() {
        anyhow::bail!("no lspee.toml found at {}", config_path.display());
    }

    let mut doc = load_doc(&config_path)?;

    let Some(lsp_item) = doc.get_mut("lsp") else {
        anyhow::bail!("no [[lsp]] entries in {}", config_path.display());
    };

    let Some(lsp_array) = lsp_item.as_array_of_tables_mut() else {
        anyhow::bail!("lsp is not an array of tables in {}", config_path.display());
    };

    let original_len = lsp_array.len();
    let mut idx = 0;
    while idx < lsp_array.len() {
        if lsp_array
            .get(idx)
            .and_then(|t| t.get("id"))
            .and_then(|v| v.as_str())
            == Some(&cmd.id)
        {
            lsp_array.remove(idx);
        } else {
            idx += 1;
        }
    }

    if lsp_array.len() == original_len {
        anyhow::bail!("no [[lsp]] entry with id '{}' found", cmd.id);
    }

    std::fs::write(&config_path, doc.to_string())?;
    println!("removed");
    Ok(())
}

fn run_set(cmd: SetCommand) -> anyhow::Result<()> {
    let project_root = canonical_root(cmd.root.as_deref())?;
    let config_path = project_root.join("lspee.toml");
    let mut doc = load_or_create_doc(&config_path)?;

    let parts: Vec<&str> = cmd.key.split('.').collect();
    let value = parse_toml_value(&cmd.value);

    match parts.as_slice() {
        [section, key] => {
            let table = doc[*section]
                .or_insert(toml_edit::Item::Table(Table::new()))
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("'{section}' is not a table in lspee.toml"))?;
            table.insert(key, toml_edit::value(value));
        }
        [key] => {
            doc[*key] = toml_edit::value(value);
        }
        _ => {
            anyhow::bail!(
                "unsupported key depth: '{}'. Use section.key format (e.g. session.idle_ttl_secs)",
                cmd.key
            );
        }
    }

    std::fs::write(&config_path, doc.to_string())?;
    println!("set {}={}", cmd.key, cmd.value);
    Ok(())
}

// --- helpers ---

fn canonical_root(override_root: Option<&Path>) -> anyhow::Result<PathBuf> {
    let root = match override_root {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir()?,
    };
    Ok(std::fs::canonicalize(&root)?)
}

fn load_doc(path: &Path) -> anyhow::Result<DocumentMut> {
    let content = std::fs::read_to_string(path)?;
    content
        .parse::<DocumentMut>()
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", path.display()))
}

fn load_or_create_doc(path: &Path) -> anyhow::Result<DocumentMut> {
    if path.exists() {
        load_doc(path)
    } else {
        Ok(DocumentMut::new())
    }
}

fn ensure_lsp_array(doc: &mut DocumentMut) -> &mut ArrayOfTables {
    if doc.get("lsp").is_none() {
        doc.insert("lsp", Item::ArrayOfTables(ArrayOfTables::new()));
    }

    doc["lsp"]
        .as_array_of_tables_mut()
        .expect("lsp should be an array of tables")
}

fn to_toml_array(items: &[String]) -> toml_edit::Array {
    let mut array = toml_edit::Array::new();
    for item in items {
        array.push(item.as_str());
    }
    array
}

fn parse_toml_value(s: &str) -> TomlValue {
    if let Ok(n) = s.parse::<i64>() {
        return TomlValue::Integer(toml_edit::Formatted::new(n));
    }
    if let Ok(b) = s.parse::<bool>() {
        return TomlValue::Boolean(toml_edit::Formatted::new(b));
    }
    TomlValue::String(toml_edit::Formatted::new(s.to_string()))
}

fn generate_init_template(project_root: &Path) -> String {
    let mut lsps = Vec::new();

    // Detect likely LSPs from project markers.
    if project_root.join("Cargo.toml").exists() {
        lsps.push(("rust-analyzer", "rust-analyzer", Vec::<&str>::new()));
    }
    if project_root.join("package.json").exists() || project_root.join("tsconfig.json").exists() {
        lsps.push((
            "typescript-language-server",
            "typescript-language-server",
            vec!["--stdio"],
        ));
    }
    if project_root.join("pyproject.toml").exists() || project_root.join("setup.py").exists() {
        lsps.push(("pyright", "pyright-langserver", vec!["--stdio"]));
    }
    if project_root.join("go.mod").exists() {
        lsps.push(("gopls", "gopls", Vec::new()));
    }

    let mut output = String::from(
        "# lspee project configuration\n\
         # Docs: https://github.com/ifiokjr/lspee\n\n",
    );

    if lsps.is_empty() {
        output.push_str(
            "# No project markers detected. Add LSP entries:\n\
             # [[lsp]]\n\
             # id = \"your-lsp\"\n\
             # command = \"your-lsp-binary\"\n\
             # args = []\n",
        );
    } else {
        for (id, command, args) in &lsps {
            output.push_str("[[lsp]]\n");
            output.push_str(&format!("id = \"{id}\"\n"));
            output.push_str(&format!("command = \"{command}\"\n"));
            if args.is_empty() {
                output.push_str("args = []\n");
            } else {
                let args_str: Vec<String> = args.iter().map(|a| format!("\"{a}\"")).collect();
                output.push_str(&format!("args = [{}]\n", args_str.join(", ")));
            }
            output.push('\n');
        }
    }

    output.push_str(
        "[session]\nidle_ttl_secs = 300\n\n\
         [memory]\nmax_session_mb = 2048\nmax_total_mb = 8192\n",
    );

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lspee-config-test-{name}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        fs::canonicalize(&dir).unwrap()
    }

    #[test]
    fn init_template_detects_rust_project() {
        let dir = temp_dir("init-rust");
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let template = generate_init_template(&dir);
        assert!(template.contains("rust-analyzer"));
        assert!(template.contains("[[lsp]]"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_template_detects_typescript_project() {
        let dir = temp_dir("init-ts");
        fs::write(dir.join("package.json"), "{}").unwrap();

        let template = generate_init_template(&dir);
        assert!(template.contains("typescript-language-server"));
        assert!(template.contains("--stdio"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_template_detects_python_project() {
        let dir = temp_dir("init-py");
        fs::write(dir.join("pyproject.toml"), "[project]\nname = \"test\"").unwrap();

        let template = generate_init_template(&dir);
        assert!(template.contains("pyright"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_template_detects_go_project() {
        let dir = temp_dir("init-go");
        fs::write(dir.join("go.mod"), "module test").unwrap();

        let template = generate_init_template(&dir);
        assert!(template.contains("gopls"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_template_shows_placeholder_for_unknown_project() {
        let dir = temp_dir("init-empty");
        let template = generate_init_template(&dir);
        assert!(template.contains("No project markers detected"));
        assert!(template.contains("# [[lsp]]"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_lsp_to_empty_config() {
        let dir = temp_dir("add-empty");
        let config_path = dir.join("lspee.toml");

        let mut doc = DocumentMut::new();
        let lsp_array = ensure_lsp_array(&mut doc);
        let mut table = Table::new();
        table.insert("id", toml_edit::value("taplo"));
        table.insert("command", toml_edit::value("taplo"));
        table.insert(
            "args",
            toml_edit::value(to_toml_array(&["lsp".into(), "stdio".into()])),
        );
        lsp_array.push(table);

        fs::write(&config_path, doc.to_string()).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("taplo"));
        assert!(content.contains("[[lsp]]"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_lsp_updates_existing_entry() {
        let dir = temp_dir("add-update");
        let config_path = dir.join("lspee.toml");

        let initial = r#"[[lsp]]
id = "taplo"
command = "old-taplo"
args = []
"#;
        fs::write(&config_path, initial).unwrap();

        let mut doc = load_doc(&config_path).unwrap();
        let lsp_array = ensure_lsp_array(&mut doc);

        for entry in lsp_array.iter_mut() {
            if entry.get("id").and_then(|v| v.as_str()) == Some("taplo") {
                entry["command"] = toml_edit::value("new-taplo");
            }
        }

        fs::write(&config_path, doc.to_string()).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("new-taplo"));
        assert!(!content.contains("old-taplo"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_lsp_from_config() {
        let dir = temp_dir("remove");
        let config_path = dir.join("lspee.toml");

        let initial = r#"[[lsp]]
id = "rust-analyzer"
command = "rust-analyzer"

[[lsp]]
id = "taplo"
command = "taplo"
"#;
        fs::write(&config_path, initial).unwrap();

        let mut doc = load_doc(&config_path).unwrap();
        let lsp_array = doc["lsp"].as_array_of_tables_mut().unwrap();

        let mut idx = 0;
        while idx < lsp_array.len() {
            if lsp_array
                .get(idx)
                .and_then(|t| t.get("id"))
                .and_then(|v| v.as_str())
                == Some("taplo")
            {
                lsp_array.remove(idx);
            } else {
                idx += 1;
            }
        }

        fs::write(&config_path, doc.to_string()).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("rust-analyzer"));
        assert!(!content.contains("taplo"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_toml_value_integers() {
        match parse_toml_value("42") {
            TomlValue::Integer(v) => assert_eq!(v.into_value(), 42),
            _ => panic!("expected integer"),
        }
    }

    #[test]
    fn parse_toml_value_booleans() {
        match parse_toml_value("true") {
            TomlValue::Boolean(v) => assert!(v.into_value()),
            _ => panic!("expected boolean"),
        }
    }

    #[test]
    fn parse_toml_value_strings() {
        match parse_toml_value("hello") {
            TomlValue::String(v) => assert_eq!(v.into_value(), "hello"),
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn set_value_in_section() {
        let dir = temp_dir("set");
        let config_path = dir.join("lspee.toml");
        fs::write(&config_path, "[session]\nidle_ttl_secs = 300\n").unwrap();

        let mut doc = load_doc(&config_path).unwrap();
        let table = doc["session"].as_table_mut().unwrap();
        table.insert("idle_ttl_secs", toml_edit::value(600));
        fs::write(&config_path, doc.to_string()).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("600"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_init_creates_file() {
        let dir = temp_dir("run-init");

        let result = run_init(InitCommand {
            root: Some(dir.clone()),
            force: false,
        });
        assert!(result.is_ok());
        assert!(dir.join("lspee.toml").exists());

        // Second call without force should fail.
        let result = run_init(InitCommand {
            root: Some(dir.clone()),
            force: false,
        });
        assert!(result.is_err());

        // With force should succeed.
        let result = run_init(InitCommand {
            root: Some(dir.clone()),
            force: true,
        });
        assert!(result.is_ok());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_add_lsp_and_remove_lsp() {
        let dir = temp_dir("run-add-remove");
        fs::write(dir.join("lspee.toml"), "").unwrap();

        run_add_lsp(AddLspCommand {
            id: "taplo".to_string(),
            command: "taplo".to_string(),
            args: Some(vec!["lsp".to_string(), "stdio".to_string()]),
            root: Some(dir.clone()),
        })
        .expect("add-lsp should succeed");

        let content = fs::read_to_string(dir.join("lspee.toml")).unwrap();
        assert!(content.contains("taplo"));

        run_remove_lsp(RemoveLspCommand {
            id: "taplo".to_string(),
            root: Some(dir.clone()),
        })
        .expect("remove-lsp should succeed");

        let content = fs::read_to_string(dir.join("lspee.toml")).unwrap();
        assert!(!content.contains("taplo"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_set_creates_section() {
        let dir = temp_dir("run-set");
        fs::write(dir.join("lspee.toml"), "").unwrap();

        run_set(SetCommand {
            key: "session.idle_ttl_secs".to_string(),
            value: "600".to_string(),
            root: Some(dir.clone()),
        })
        .expect("set should succeed");

        let content = fs::read_to_string(dir.join("lspee.toml")).unwrap();
        assert!(content.contains("[session]"));
        assert!(content.contains("600"));

        let _ = fs::remove_dir_all(&dir);
    }
}
