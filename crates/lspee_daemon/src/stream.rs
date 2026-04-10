use std::{path::Path, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    sync::{Mutex, mpsc},
};

use crate::{SessionHandle, SessionRegistry};

pub async fn spawn_dedicated_stream_endpoint(
    project_root: &Path,
    lease_id: &str,
    handle: SessionHandle,
    registry: SessionRegistry,
) -> Result<PathBuf> {
    let stream_dir = project_root.join(".lspee").join("streams");
    tokio::fs::create_dir_all(&stream_dir)
        .await
        .context("failed to create stream socket directory")?;

    let endpoint = stream_dir.join(format!("{lease_id}.sock"));
    if tokio::fs::try_exists(&endpoint).await.unwrap_or(false) {
        let _ = tokio::fs::remove_file(&endpoint).await;
    }

    let listener = UnixListener::bind(&endpoint).with_context(|| {
        format!(
            "failed to bind dedicated stream socket {}",
            endpoint.display()
        )
    })?;
    let endpoint_for_task = endpoint.clone();
    let lease_id = lease_id.to_string();

    tokio::spawn(async move {
        let result = async {
            let (stream, _) = listener.accept().await?;
            run_stream_connection(stream, lease_id.clone(), handle, registry.clone()).await
        }
        .await;

        if let Err(error) = result {
            tracing::warn!(path = %endpoint_for_task.display(), ?error, "dedicated stream endpoint failed");
        }

        let _ = tokio::fs::remove_file(&endpoint_for_task).await;
    });

    Ok(endpoint)
}

async fn run_stream_connection(
    stream: tokio::net::UnixStream,
    lease_id: String,
    handle: SessionHandle,
    registry: SessionRegistry,
) -> Result<()> {
    let (reader, writer) = stream.into_split();
    let writer = Arc::new(Mutex::new(writer));
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Vec<u8>>(256);

    let writer_task = {
        let writer = Arc::clone(&writer);
        tokio::spawn(async move {
            while let Some(frame) = outbound_rx.recv().await {
                let mut writer = writer.lock().await;
                writer.write_all(&frame).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
            }
            Ok::<(), anyhow::Error>(())
        })
    };

    let runtime_task = {
        let outbound_tx = outbound_tx.clone();
        let lease_id = lease_id.clone();
        let mut runtime_rx = handle.runtime.subscribe();
        tokio::spawn(async move {
            let mut seq = 1_u64;
            loop {
                match runtime_rx.recv().await {
                    Ok(message) => {
                        let frame = crate::protocol::StreamFrame {
                            v: crate::PROTOCOL_VERSION,
                            frame_type: crate::protocol::StreamFrameType::LspOut,
                            lease_id: lease_id.clone(),
                            seq,
                            payload: message.payload,
                        };
                        seq = seq.saturating_add(1);
                        outbound_tx.send(serde_json::to_vec(&frame)?).await?;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            Ok::<(), anyhow::Error>(())
        })
    };

    let event_task = {
        let outbound_tx = outbound_tx.clone();
        let lease_id = lease_id.clone();
        let mut event_rx = handle.events.subscribe();
        tokio::spawn(async move {
            let seq = 1_u64;
            loop {
                match event_rx.recv().await {
                    Ok(payload) => {
                        let frame = crate::protocol::StreamFrame {
                            v: crate::PROTOCOL_VERSION,
                            frame_type: crate::protocol::StreamFrameType::StreamError,
                            lease_id: lease_id.clone(),
                            seq,
                            payload,
                        };
                        outbound_tx.send(serde_json::to_vec(&frame)?).await?;
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            Ok::<(), anyhow::Error>(())
        })
    };

    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let frame: crate::protocol::StreamFrame<Value> = serde_json::from_str(&line)
            .context("failed to decode inbound dedicated stream frame")?;

        if frame.lease_id != lease_id {
            continue;
        }

        match frame.frame_type {
            crate::protocol::StreamFrameType::LspIn => {
                handle.runtime.send(frame.payload).await?;
            }
            crate::protocol::StreamFrameType::LspOut
            | crate::protocol::StreamFrameType::StreamError => {}
        }
    }

    runtime_task.abort();
    event_task.abort();
    drop(outbound_tx);
    let _ = writer_task.await;
    let _ = registry.release_by_lease_id(&lease_id).await;
    Ok(())
}
