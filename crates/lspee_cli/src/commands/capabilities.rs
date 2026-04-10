use clap::{Args, ValueEnum};
use lspee_config::resolve;
use lspee_daemon::{
    Attach, AttachCapabilities, AttachOk, ClientKind, ClientMeta, ControlEnvelope, Release,
    SessionKeyWire, StreamMode, TYPE_ATTACH, TYPE_ATTACH_OK, TYPE_RELEASE, TYPE_RELEASE_OK,
};
use serde_json::Value;
use std::{path::PathBuf, process};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::client;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CapabilitiesOutput {
    /// Human-readable summary of supported methods.
    Human,
    /// Compact JSON output for automation/agents.
    Json,
}

#[derive(Debug, Args)]
pub struct CapabilitiesCommand {
    /// LSP server identifier to query (e.g. rust-analyzer).
    #[arg(long = "lsp")]
    pub lsp: String,

    /// Override project root used for config resolution and session identity.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Disable daemon auto-start when socket is missing.
    #[arg(long)]
    pub no_start_daemon: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t = CapabilitiesOutput::Json)]
    pub output: CapabilitiesOutput,
}

pub fn run(cmd: CapabilitiesCommand) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_async(cmd))
}

async fn run_async(cmd: CapabilitiesCommand) -> anyhow::Result<()> {
    let resolved = resolve(cmd.root.as_deref())?;

    let stream = client::connect(&resolved.project_root, !cmd.no_start_daemon).await?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let attach_id = client::new_request_id("attach");
    let attach = ControlEnvelope {
        v: lspee_daemon::PROTOCOL_VERSION,
        id: Some(attach_id.clone()),
        message_type: TYPE_ATTACH.to_string(),
        payload: serde_json::to_value(Attach {
            session_key: SessionKeyWire {
                project_root: resolved.project_root.display().to_string(),
                config_hash: resolved.config_hash,
                lsp_id: cmd.lsp.clone(),
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

    client::write_frame(&mut writer, &attach).await?;
    let attach_response = client::read_response_for_id(&mut lines, &attach_id).await?;
    client::ensure_not_error(&attach_response)?;
    if attach_response.message_type != TYPE_ATTACH_OK {
        anyhow::bail!(
            "unexpected response type for Attach: {}",
            attach_response.message_type
        );
    }

    let attach_ok: AttachOk = serde_json::from_value(attach_response.payload)
        .map_err(|e| anyhow::anyhow!("invalid AttachOk payload: {e}"))?;

    let lease_id = attach_ok.lease_id.clone();
    let capabilities = extract_capabilities(&cmd.lsp, attach_ok.initialize_result.as_ref());

    // Release the lease immediately — we only needed the initialize_result.
    let release_result = release_lease(&mut writer, &mut lines, &lease_id).await;
    if let Err(error) = release_result {
        tracing::warn!(?error, lease_id, "failed to release lease after capabilities query");
    }

    match cmd.output {
        CapabilitiesOutput::Human => print_human(&cmd.lsp, &capabilities),
        CapabilitiesOutput::Json => {
            println!("{}", serde_json::to_string(&capabilities)?);
        }
    }

    Ok(())
}

fn extract_capabilities(lsp_id: &str, initialize_result: Option<&Value>) -> Value {
    let Some(init) = initialize_result else {
        return serde_json::json!({
            "lsp_id": lsp_id,
            "error": "no initialize_result available",
            "methods": {},
            "raw": null,
        });
    };

    let caps = init
        .get("result")
        .and_then(|r| r.get("capabilities"))
        .or_else(|| init.get("capabilities"));

    let Some(caps) = caps else {
        return serde_json::json!({
            "lsp_id": lsp_id,
            "error": "no capabilities field in initialize result",
            "methods": {},
            "raw": init,
        });
    };

    let methods = serde_json::json!({
        "textDocument/completion": has_capability(caps, "completionProvider"),
        "textDocument/hover": has_capability(caps, "hoverProvider"),
        "textDocument/signatureHelp": has_capability(caps, "signatureHelpProvider"),
        "textDocument/declaration": has_capability(caps, "declarationProvider"),
        "textDocument/definition": has_capability(caps, "definitionProvider"),
        "textDocument/typeDefinition": has_capability(caps, "typeDefinitionProvider"),
        "textDocument/implementation": has_capability(caps, "implementationProvider"),
        "textDocument/references": has_capability(caps, "referencesProvider"),
        "textDocument/documentHighlight": has_capability(caps, "documentHighlightProvider"),
        "textDocument/documentSymbol": has_capability(caps, "documentSymbolProvider"),
        "textDocument/codeAction": has_capability(caps, "codeActionProvider"),
        "textDocument/codeLens": has_capability(caps, "codeLensProvider"),
        "textDocument/documentLink": has_capability(caps, "documentLinkProvider"),
        "textDocument/colorPresentation": has_capability(caps, "colorProvider"),
        "textDocument/formatting": has_capability(caps, "documentFormattingProvider"),
        "textDocument/rangeFormatting": has_capability(caps, "documentRangeFormattingProvider"),
        "textDocument/onTypeFormatting": has_capability(caps, "documentOnTypeFormattingProvider"),
        "textDocument/rename": has_capability(caps, "renameProvider"),
        "textDocument/foldingRange": has_capability(caps, "foldingRangeProvider"),
        "textDocument/selectionRange": has_capability(caps, "selectionRangeProvider"),
        "textDocument/linkedEditingRange": has_capability(caps, "linkedEditingRangeProvider"),
        "textDocument/prepareCallHierarchy": has_capability(caps, "callHierarchyProvider"),
        "textDocument/prepareTypeHierarchy": has_capability(caps, "typeHierarchyProvider"),
        "textDocument/semanticTokens": has_capability(caps, "semanticTokensProvider"),
        "textDocument/moniker": has_capability(caps, "monikerProvider"),
        "textDocument/inlayHint": has_capability(caps, "inlayHintProvider"),
        "textDocument/inlineValue": has_capability(caps, "inlineValueProvider"),
        "textDocument/diagnostic": has_capability(caps, "diagnosticProvider"),
        "workspace/symbol": has_capability(caps, "workspaceSymbolProvider"),
    });

    let server_info = init
        .get("result")
        .and_then(|r| r.get("serverInfo"))
        .or_else(|| init.get("serverInfo"))
        .cloned();

    serde_json::json!({
        "lsp_id": lsp_id,
        "server_info": server_info,
        "methods": methods,
        "raw_capabilities": caps,
    })
}

fn has_capability(caps: &Value, key: &str) -> bool {
    match caps.get(key) {
        None => false,
        Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        // Any non-null object/value means the capability is present.
        Some(_) => true,
    }
}

fn print_human(lsp_id: &str, capabilities: &Value) {
    println!("lsp_id={lsp_id}");

    if let Some(info) = capabilities.get("server_info") {
        if let Some(name) = info.get("name").and_then(Value::as_str) {
            print!("server={name}");
            if let Some(version) = info.get("version").and_then(Value::as_str) {
                print!(" v{version}");
            }
            println!();
        }
    }

    if let Some(error) = capabilities.get("error").and_then(Value::as_str) {
        println!("error={error}");
        return;
    }

    if let Some(methods) = capabilities.get("methods").and_then(Value::as_object) {
        let mut supported: Vec<&str> = Vec::new();
        let mut unsupported: Vec<&str> = Vec::new();

        for (method, available) in methods {
            if available.as_bool().unwrap_or(false) {
                supported.push(method);
            } else {
                unsupported.push(method);
            }
        }

        supported.sort_unstable();
        unsupported.sort_unstable();

        println!("supported_methods={}", supported.len());
        for method in &supported {
            println!("  + {method}");
        }
        if !unsupported.is_empty() {
            println!("unsupported_methods={}", unsupported.len());
            for method in &unsupported {
                println!("  - {method}");
            }
        }
    }
}

async fn release_lease(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    lease_id: &str,
) -> anyhow::Result<()> {
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
    let release_response = client::read_response_for_id(lines, &release_id).await?;
    client::ensure_not_error(&release_response)?;
    if release_response.message_type != TYPE_RELEASE_OK {
        anyhow::bail!(
            "unexpected response type for Release: {}",
            release_response.message_type
        );
    }

    Ok(())
}
