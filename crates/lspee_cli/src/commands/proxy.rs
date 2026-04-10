use clap::Args;
use lspee_config::resolve;
use lspee_daemon::{
    Attach, AttachCapabilities, ClientKind, ClientMeta, ControlEnvelope, SessionKeyWire,
    StreamErrorPayload, StreamFrame, StreamFrameType, StreamMode, TYPE_ATTACH, TYPE_ATTACH_OK,
};
use serde_json::{Value, json};
use std::{path::PathBuf, process};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

use super::client;

#[derive(Debug, Args)]
pub struct ProxyCommand {
    /// LSP server identifier to target (e.g. rust-analyzer).
    #[arg(long = "lsp")]
    pub lsp: String,

    /// Override project root used for config resolution and session identity.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Disable daemon auto-start when socket is missing.
    #[arg(long)]
    pub no_start_daemon: bool,
}

pub fn run(cmd: ProxyCommand) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_async(cmd))
}

async fn run_async(cmd: ProxyCommand) -> anyhow::Result<()> {
    let resolved = resolve(cmd.root.as_deref())?;

    let control_stream = client::connect(&resolved.project_root, !cmd.no_start_daemon).await?;
    let (control_reader, mut control_writer) = control_stream.into_split();
    let mut control_lines = BufReader::new(control_reader).lines();

    let attach_id = client::new_request_id("attach");
    let attach = ControlEnvelope {
        v: lspee_daemon::PROTOCOL_VERSION,
        id: Some(attach_id.clone()),
        message_type: TYPE_ATTACH.to_string(),
        payload: serde_json::to_value(Attach {
            session_key: SessionKeyWire {
                project_root: resolved.project_root.display().to_string(),
                config_hash: resolved.config_hash,
                lsp_id: cmd.lsp,
            },
            client_meta: ClientMeta {
                client_name: "lspee_proxy".to_string(),
                client_version: env!("CARGO_PKG_VERSION").to_string(),
                client_kind: Some(ClientKind::Editor),
                pid: Some(process::id()),
                cwd: std::env::current_dir()
                    .ok()
                    .map(|cwd| cwd.display().to_string()),
            },
            capabilities: Some(AttachCapabilities {
                stream_mode: vec![StreamMode::Dedicated],
            }),
        })?,
    };

    client::write_frame(&mut control_writer, &attach).await?;
    let attach_response = client::read_response_for_id(&mut control_lines, &attach_id).await?;
    client::ensure_not_error(&attach_response)?;

    if attach_response.message_type != TYPE_ATTACH_OK {
        anyhow::bail!(
            "unexpected response type for Attach: {}",
            attach_response.message_type
        );
    }

    let lease_id = attach_response
        .payload
        .get("lease_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("AttachOk missing lease_id"))?
        .to_string();
    let initialize_result = attach_response
        .payload
        .get("initialize_result")
        .cloned()
        .unwrap_or(Value::Null);
    let endpoint = attach_response
        .payload
        .get("stream")
        .and_then(|stream| stream.get("endpoint"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("AttachOk missing stream endpoint"))?;
    let endpoint = endpoint
        .strip_prefix("unix://")
        .ok_or_else(|| anyhow::anyhow!("unsupported stream endpoint: {endpoint}"))?;

    // Control connection no longer needed once dedicated stream is established.
    drop(control_writer);
    drop(control_lines);

    let stream = UnixStream::connect(endpoint).await.map_err(|error| {
        anyhow::anyhow!(
            "failed to connect to dedicated stream endpoint {}: {error}",
            endpoint
        )
    })?;
    let (stream_reader, mut stream_writer) = stream.into_split();
    let mut stream_lines = BufReader::new(stream_reader).lines();

    let mut stdin_reader = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut seq = 1_u64;

    loop {
        tokio::select! {
            incoming = lspee_lsp::read_lsp_frame(&mut stdin_reader) => {
                match incoming? {
                    Some(message) => {
                        if handle_editor_message(
                            &message,
                            &initialize_result,
                            &lease_id,
                            &mut seq,
                            &mut stream_writer,
                            &mut stdout,
                        ).await? {
                            break;
                        }
                    }
                    None => break,
                }
            }
            next = stream_lines.next_line() => {
                let Some(line) = next? else {
                    break;
                };

                let frame: StreamFrame<Value> = serde_json::from_str(&line)
                    .map_err(|error| anyhow::anyhow!("invalid stream frame from daemon: {error}"))?;

                match frame.frame_type {
                    StreamFrameType::LspOut => {
                        write_lsp_message(&mut stdout, &frame.payload).await?;
                    }
                    StreamFrameType::StreamError => {
                        let payload: StreamErrorPayload = serde_json::from_value(frame.payload)
                            .map_err(|error| anyhow::anyhow!("invalid stream error payload: {error}"))?;
                        let warning = stream_error_to_editor_warning(&payload);
                        write_lsp_message(&mut stdout, &warning).await?;
                        break;
                    }
                    StreamFrameType::LspIn => {}
                }
            }
        }
    }

    Ok(())
}

async fn handle_editor_message(
    message: &Value,
    initialize_result: &Value,
    lease_id: &str,
    seq: &mut u64,
    stream_writer: &mut tokio::net::unix::OwnedWriteHalf,
    stdout: &mut tokio::io::Stdout,
) -> anyhow::Result<bool> {
    let method = message.get("method").and_then(Value::as_str);
    let id = message.get("id").cloned();

    match (method, id) {
        (Some("initialize"), Some(id)) => {
            let response = initialize_response_for_editor(id, initialize_result);
            write_lsp_message(stdout, &response).await?;
            Ok(false)
        }
        (Some("shutdown"), Some(id)) => {
            let response = shutdown_response_for_editor(id);
            write_lsp_message(stdout, &response).await?;
            Ok(false)
        }
        (Some("initialized"), None) => Ok(false),
        (Some("exit"), None) => Ok(true),
        _ => {
            let frame = StreamFrame {
                v: lspee_daemon::PROTOCOL_VERSION,
                frame_type: StreamFrameType::LspIn,
                lease_id: lease_id.to_string(),
                seq: *seq,
                payload: message.clone(),
            };
            *seq = seq.saturating_add(1);

            let mut encoded = serde_json::to_vec(&frame)?;
            encoded.push(b'\n');
            stream_writer.write_all(&encoded).await?;
            stream_writer.flush().await?;
            Ok(false)
        }
    }
}

async fn write_lsp_message(stdout: &mut tokio::io::Stdout, message: &Value) -> anyhow::Result<()> {
    let frame = lspee_lsp::encode_lsp_frame(message)?;
    stdout.write_all(&frame).await?;
    stdout.flush().await?;
    Ok(())
}

fn initialize_response_for_editor(id: Value, initialize_result: &Value) -> Value {
    let result = initialize_result
        .get("result")
        .cloned()
        .unwrap_or_else(|| initialize_result.clone());

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn shutdown_response_for_editor(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": null,
    })
}

fn stream_error_to_editor_warning(payload: &StreamErrorPayload) -> Value {
    let resume_hint = payload
        .details
        .as_ref()
        .and_then(|details| details.get("resume_hint"))
        .and_then(Value::as_str)
        .unwrap_or("Retry attaching to the daemon session.");

    json!({
        "jsonrpc": "2.0",
        "method": "window/showMessage",
        "params": {
            "type": 2,
            "message": format!("lspee warning: {} {}", payload.message, resume_hint),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        initialize_response_for_editor, shutdown_response_for_editor,
        stream_error_to_editor_warning,
    };
    use lspee_daemon::StreamErrorPayload;
    use serde_json::json;

    #[test]
    fn initialize_response_reuses_backend_result() {
        let response = initialize_response_for_editor(
            json!(1),
            &json!({
                "jsonrpc": "2.0",
                "id": "lspee-initialize",
                "result": {"capabilities": {"hoverProvider": true}}
            }),
        );

        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["capabilities"]["hoverProvider"], true);
    }

    #[test]
    fn shutdown_response_returns_null_result() {
        let response = shutdown_response_for_editor(json!(7));
        assert_eq!(response["id"], 7);
        assert!(response["result"].is_null());
    }

    #[test]
    fn stream_error_warning_includes_resume_hint() {
        let warning = stream_error_to_editor_warning(&StreamErrorPayload {
            code: "E_SESSION_EVICTED_MEMORY".to_string(),
            message: "memory pressure".to_string(),
            retryable: true,
            details: Some(json!({"resume_hint": "Restart the LSP in Helix."})),
        });

        assert_eq!(warning["method"], "window/showMessage");
        assert!(
            warning["params"]["message"]
                .as_str()
                .expect("warning message should be string")
                .contains("Restart the LSP in Helix.")
        );
    }
}
