#![cfg(unix)]

//! Snapshot-based integration tests using fixtures and insta.

use std::path::Path;
use tokio::io::AsyncBufReadExt;

use lspee_test_helpers::daemon::DaemonHandle;
use lspee_test_helpers::fixtures::setup_fixture;
use lspee_test_helpers::snapshots::snapshot_settings;

// ---------------------------------------------------------------------------
// Helper: run a CLI command and capture the result
// ---------------------------------------------------------------------------

fn run_lsp_json(root: &Path) -> anyhow::Result<()> {
	lspee_cli::commands::lsp::run(&lspee_cli::commands::lsp::LspCommand {
		project_root: Some(root.to_path_buf()),
		output: lspee_cli::commands::lsp::LspOutput::Json,
	})
}

fn run_config_show_json(root: &Path) -> anyhow::Result<()> {
	lspee_cli::commands::config::run(&lspee_cli::commands::config::ConfigCommand {
		action: lspee_cli::commands::config::ConfigAction::Show(
			lspee_cli::commands::config::ShowCommand {
				root: Some(root.to_path_buf()),
				output: lspee_cli::commands::config::ConfigOutput::Json,
			},
		),
	})
}

// ---------------------------------------------------------------------------
// Fixture-based tests
// ---------------------------------------------------------------------------

#[test]
fn lsp_command_with_rust_fixture() {
	let (_temp, root) = setup_fixture("rust-project");
	let result = run_lsp_json(&root);
	assert!(result.is_ok());
}

#[test]
fn config_show_with_multi_lsp_fixture() {
	let (_temp, root) = setup_fixture("multi-lsp-project");

	let resolved = lspee_config::resolve(Some(&root)).unwrap();
	assert_eq!(resolved.merged.lsps.len(), 2);
	assert!(resolved.merged.lsp_config("rust-analyzer").is_some());
	assert!(resolved.merged.lsp_config("taplo").is_some());
	assert_eq!(resolved.merged.session.idle_ttl_secs, 600);

	let result = run_config_show_json(&root);
	assert!(result.is_ok());
}

#[test]
fn config_show_with_empty_project_fixture() {
	let (_temp, root) = setup_fixture("empty-project");
	let resolved = lspee_config::resolve(Some(&root)).unwrap();
	assert!(resolved.merged.lsps.is_empty());
}

// ---------------------------------------------------------------------------
// Daemon tests with fixtures and DaemonHandle
// ---------------------------------------------------------------------------

#[test]
fn status_via_daemon_handle() {
	let (_temp, root) = setup_fixture("rust-project");
	let _daemon = DaemonHandle::start(&root);

	let result = lspee_cli::commands::status::run(lspee_cli::commands::status::StatusCommand {
		project_root: Some(root.clone()),
		no_start_daemon: false,
		output: lspee_cli::commands::status::StatusOutput::Json,
	});
	assert!(result.is_ok());
	// DaemonHandle drops and shuts down automatically
}

#[test]
fn call_via_daemon_handle() {
	let (_temp, root) = setup_fixture("rust-project");
	let _daemon = DaemonHandle::start(&root);

	let request = serde_json::json!({
		"jsonrpc": "2.0",
		"id": 1,
		"method": "textDocument/hover",
		"params": {}
	});

	let result = lspee_cli::commands::call::run(lspee_cli::commands::call::CallCommand {
		lsp: "rust-analyzer".to_string(),
		root: Some(root.clone()),
		request: serde_json::to_string(&request).unwrap(),
		no_start_daemon: false,
		client_kind: lspee_cli::commands::call::CallClientKind::Agent,
		output: lspee_cli::commands::call::CallOutput::Json,
	});
	assert!(result.is_ok());
}

#[test]
fn capabilities_via_daemon_handle() {
	let (_temp, root) = setup_fixture("rust-project");
	let _daemon = DaemonHandle::start(&root);

	let result = lspee_cli::commands::capabilities::run(
		lspee_cli::commands::capabilities::CapabilitiesCommand {
			lsp: "rust-analyzer".to_string(),
			root: Some(root.clone()),
			no_start_daemon: false,
			output: lspee_cli::commands::capabilities::CapabilitiesOutput::Json,
		},
	);
	assert!(result.is_ok());

	// Also test human output
	let result = lspee_cli::commands::capabilities::run(
		lspee_cli::commands::capabilities::CapabilitiesCommand {
			lsp: "rust-analyzer".to_string(),
			root: Some(root.clone()),
			no_start_daemon: false,
			output: lspee_cli::commands::capabilities::CapabilitiesOutput::Human,
		},
	);
	assert!(result.is_ok());
}

#[test]
fn stop_via_daemon_handle() {
	let (_temp, root) = setup_fixture("rust-project");
	let daemon = DaemonHandle::start(&root);

	let result = lspee_cli::commands::stop::run(lspee_cli::commands::stop::StopCommand {
		project_root: Some(root.clone()),
	});
	assert!(result.is_ok());

	// Explicitly stop so the handle doesn't try to shut down a dead daemon
	drop(daemon);
}

#[test]
fn multi_lsp_daemon_handles_both_lsps() {
	let (_temp, root) = setup_fixture("multi-lsp-project");
	let _daemon = DaemonHandle::start(&root);

	// Call rust-analyzer
	let request = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"test","params":{}});
	let result = lspee_cli::commands::call::run(lspee_cli::commands::call::CallCommand {
		lsp: "rust-analyzer".to_string(),
		root: Some(root.clone()),
		request: serde_json::to_string(&request).unwrap(),
		no_start_daemon: false,
		client_kind: lspee_cli::commands::call::CallClientKind::Agent,
		output: lspee_cli::commands::call::CallOutput::Json,
	});
	assert!(result.is_ok());

	// Call taplo
	let result = lspee_cli::commands::call::run(lspee_cli::commands::call::CallCommand {
		lsp: "taplo".to_string(),
		root: Some(root.clone()),
		request: serde_json::to_string(&request).unwrap(),
		no_start_daemon: false,
		client_kind: lspee_cli::commands::call::CallClientKind::Agent,
		output: lspee_cli::commands::call::CallOutput::Json,
	});
	assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// Snapshot test for insta integration
// ---------------------------------------------------------------------------

#[test]
fn snapshot_resolved_config_structure() {
	let (_temp, root) = setup_fixture("multi-lsp-project");
	let resolved = lspee_config::resolve(Some(&root)).unwrap();

	let settings = snapshot_settings();
	settings.bind(|| {
		insta::assert_json_snapshot!(
			"resolved_config",
			serde_json::json!({
				"lsps": resolved.merged.lsps,
				"root_markers": resolved.merged.root_markers,
				"workspace_mode": resolved.merged.workspace_mode,
				"session": {
					"idle_ttl_secs": resolved.merged.session.idle_ttl_secs,
				},
			})
		);
	});
}

// ---------------------------------------------------------------------------
// Client write/read integration tests via daemon
// ---------------------------------------------------------------------------

#[test]
fn client_connect_write_read_roundtrip() {
	let (_temp, root) = setup_fixture("rust-project");
	let _daemon = DaemonHandle::start(&root);

	// Use the actual client module functions for connect + write + read
	let rt = tokio::runtime::Runtime::new().unwrap();
	rt.block_on(async {
		let stream = lspee_cli::commands::client::connect(&root, false)
			.await
			.expect("should connect to daemon");
		let (reader, mut writer) = stream.into_split();
		let mut lines = tokio::io::BufReader::new(reader).lines();

		let req_id = lspee_cli::commands::client::new_request_id("test");
		let request = lspee_daemon::ControlEnvelope {
			v: lspee_daemon::PROTOCOL_VERSION,
			id: Some(req_id.clone()),
			message_type: lspee_daemon::TYPE_STATS.to_string(),
			payload: serde_json::to_value(lspee_daemon::Stats::default()).unwrap(),
		};

		lspee_cli::commands::client::write_frame(&mut writer, &request)
			.await
			.expect("should write frame");

		let response = lspee_cli::commands::client::read_response_for_id(&mut lines, &req_id)
			.await
			.expect("should read response");

		lspee_cli::commands::client::ensure_not_error(&response).expect("should not be error");
		assert_eq!(response.message_type, lspee_daemon::TYPE_STATS_OK);
	});
}

#[test]
fn client_connect_no_auto_start_fails_without_daemon() {
	let (_temp, root) = setup_fixture("rust-project");
	// No daemon started — connect with auto_start=false should fail
	let rt = tokio::runtime::Runtime::new().unwrap();
	let result = rt.block_on(async { lspee_cli::commands::client::connect(&root, false).await });
	assert!(result.is_err());
}
