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
        if lsp_array.get(idx).and_then(|t| t.get("id")).and_then(|v| v.as_str())
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
                .ok_or_else(|| {
                    anyhow::anyhow!("'{section}' is not a table in lspee.toml")
                })?;
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

    // If lsp is a regular table (old single format), convert to array.
    if doc.get("lsp").is_some_and(|v| v.is_table()) {
        let old_table = doc.remove("lsp").expect("lsp should exist");
        let mut array = ArrayOfTables::new();
        if let Item::Table(t) = old_table {
            array.push(t);
        }
        doc.insert("lsp", Item::ArrayOfTables(array));
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
    if project_root.join("package.json").exists()
        || project_root.join("tsconfig.json").exists()
    {
        lsps.push((
            "typescript-language-server",
            "typescript-language-server",
            vec!["--stdio"],
        ));
    }
    if project_root.join("pyproject.toml").exists()
        || project_root.join("setup.py").exists()
    {
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
                let args_str: Vec<String> =
                    args.iter().map(|a| format!("\"{a}\"")).collect();
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
