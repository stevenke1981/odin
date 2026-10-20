//! These tests verify protocol handling against a fixture, NOT real backend parity.
use odin_core::{
    config::{CommandConfig, LanguageConfig},
    paths::SafeRoot,
};
use odin_protocol::{
    dap::DebugAdapter,
    lsp::LspClient,
    rpc::{Dialect, Framing, RpcProcess},
};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
fn python() -> String {
    std::env::var("ODIN_PYTHON").unwrap_or_else(|_| {
        if cfg!(windows) {
            "python".into()
        } else {
            "python3".into()
        }
    })
}
fn args(mode: &str) -> Vec<String> {
    vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/protocol_peer.py")
            .to_string_lossy()
            .into_owned(),
        mode.into(),
    ]
}
#[tokio::test]
async fn lsp_fixture_lifecycle_and_symbols() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("lib.rs"), "fn foo() {}\n").unwrap();
    let language = LanguageConfig {
        id: "rust".into(),
        extensions: vec!["rs".into()],
        program: python(),
        args: args("lsp"),
        initialization_options: serde_json::Value::Null,
        settings: serde_json::Value::Null,
    };
    let client = LspClient::start(
        SafeRoot::new(d.path(), &[], 10000).unwrap(),
        language,
        Duration::from_secs(3),
    )
    .await
    .unwrap();
    assert_eq!(client.symbols("lib.rs").await.unwrap()[0].name, "foo");
    assert_eq!(
        client.diagnostics("lib.rs").await.unwrap()["available"],
        true
    );
}
#[tokio::test]
async fn dap_launch_does_not_block_configuration_done() {
    let d = tempfile::tempdir().unwrap();
    let config = CommandConfig {
        program: python(),
        args: args("dap"),
        timeout_secs: 3,
    };
    let adapter = DebugAdapter::start(&config, &SafeRoot::new(d.path(), &[], 1000).unwrap())
        .await
        .unwrap();
    assert_eq!(
        adapter
            .request("launch", json!({"program":"fixture"}))
            .await
            .unwrap()["pending"],
        true
    );
    adapter
        .request("configurationDone", json!({}))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    let result = adapter.poll().await.unwrap();
    assert_eq!(result["launch_results"].as_array().unwrap().len(), 1);
}
#[tokio::test]
async fn backend_timeout_is_reported_not_retried() {
    let d = tempfile::tempdir().unwrap();
    let rpc = RpcProcess::spawn(
        &python(),
        &args("lsp"),
        d.path(),
        Framing::ContentLength,
        Dialect::JsonRpc,
        Duration::from_millis(100),
        json!([]),
        json!({}),
    )
    .unwrap();
    let error = rpc.request("fixture/hang", json!({})).await.unwrap_err();
    assert!(error.to_string().contains("NEVER auto-retried"));
}
#[tokio::test]
#[ignore = "Requires installed rust-analyzer, Rust sysroot, and real fixture project; run explicitly"]
async fn real_rust_analyzer_smoke() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/rust-fixture")
        .canonicalize()
        .unwrap();
    let config = LanguageConfig {
        id: "rust".into(),
        extensions: vec!["rs".into()],
        program: "rust-analyzer".into(),
        args: vec![],
        initialization_options: serde_json::Value::Null,
        settings: serde_json::Value::Null,
    };
    let client = LspClient::start(
        SafeRoot::new(root, &[], 100000).unwrap(),
        config,
        Duration::from_secs(120),
    )
    .await
    .unwrap();
    let symbols = client.symbols("src/lib.rs").await.unwrap();
    assert!(symbols.iter().any(|s| s.name == "greet"));
}

async fn client_with(root: &std::path::Path, options: &[&str]) -> std::sync::Arc<LspClient> {
    let mut arguments = args("lsp");
    arguments.extend(options.iter().map(|s| s.to_string()));
    let language = LanguageConfig {
        id: "rust".into(),
        extensions: vec!["rs".into()],
        program: python(),
        args: arguments,
        initialization_options: serde_json::Value::Null,
        settings: serde_json::Value::Null,
    };
    LspClient::start(
        SafeRoot::new(root, &[], 10000).unwrap(),
        language,
        Duration::from_secs(3),
    )
    .await
    .unwrap()
}

async fn wait_phase(adapter: &DebugAdapter, phase: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let status = adapter.status().unwrap();
        if status["phase"] == phase {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Expected {phase}, received {status}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn lsp_incremental_sync_uses_utf16_old_document_range() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("lib.rs"), "fn foo() {}\r\n// 😀\r\n").unwrap();
    let client = client_with(temp.path(), &["--sync-kind", "2"]).await;
    assert_eq!(client.symbols("lib.rs").await.unwrap()[0].name, "foo");
    std::fs::write(temp.path().join("lib.rs"), "fn bar() {}\r// 中😀\r").unwrap();
    assert_eq!(client.symbols("lib.rs").await.unwrap()[0].name, "bar");
    client.shutdown().await.unwrap();
    assert!(client.is_closed());
    assert!(
        client
            .symbols("lib.rs")
            .await
            .unwrap_err()
            .to_string()
            .contains("restart")
    );
}

#[tokio::test]
async fn lsp_open_close_only_sync_reopens_changed_document() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
    let client = client_with(temp.path(), &["--sync-kind", "0"]).await;
    assert_eq!(client.symbols("lib.rs").await.unwrap()[0].name, "foo");
    std::fs::write(temp.path().join("lib.rs"), "fn changed() {}").unwrap();
    assert_eq!(client.symbols("lib.rs").await.unwrap()[0].name, "changed");
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn stale_push_diagnostics_are_pending_until_current_version_arrives() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
    let client = client_with(temp.path(), &["--push-only", "--stale-diagnostics"]).await;
    client.symbols("lib.rs").await.unwrap();
    assert_eq!(
        client.diagnostics("lib.rs").await.unwrap()["version_verified"],
        true
    );
    std::fs::write(temp.path().join("lib.rs"), "fn changed() {}").unwrap();
    client.symbols("lib.rs").await.unwrap();
    let stale = client.diagnostics("lib.rs").await.unwrap();
    assert_eq!(stale["available"], false);
    assert_eq!(stale["stale"], true);
    let old_version = stale["current_version"].as_i64().unwrap();
    std::fs::remove_file(temp.path().join("lib.rs")).unwrap();
    client.refresh(&["lib.rs".into()]).await.unwrap();
    std::fs::write(temp.path().join("lib.rs"), "fn reopened() {}").unwrap();
    client.symbols("lib.rs").await.unwrap();
    let current = client.diagnostics("lib.rs").await.unwrap();
    assert_eq!(current["version_verified"], true);
    assert!(current["current_version"].as_i64().unwrap() > old_version);
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn code_action_resolution_preserves_diagnostics_without_executing_edits() {
    use odin_protocol::text::{Position, Range};
    let temp = tempfile::tempdir().unwrap();
    let before = "fn foo() {}";
    std::fs::write(temp.path().join("lib.rs"), before).unwrap();
    let client = client_with(temp.path(), &[]).await;
    let range = Range {
        start: Position {
            line: 0,
            character: 3,
        },
        end: Position {
            line: 0,
            character: 6,
        },
    };
    let diagnostic = json!({"range":range,"message":"fixture warning","severity":2});
    let actions = client
        .actions_with_diagnostics("lib.rs", range, std::slice::from_ref(&diagnostic))
        .await
        .unwrap();
    assert_eq!(actions[0]["data"]["diagnostics"][0], diagnostic);
    let action = client.resolve_action(&actions[0]).await.unwrap();
    let snapshots = std::collections::BTreeMap::from([("lib.rs".into(), before.into())]);
    let changes = client.changes(&action["edit"], &snapshots).await.unwrap();
    assert_eq!(changes[0].content.as_deref(), Some("fn resolved() {}"));
    assert_eq!(
        std::fs::read_to_string(temp.path().join("lib.rs")).unwrap(),
        before
    );
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn request_timeout_cancels_once_and_leaves_no_pending_registration() {
    let temp = tempfile::tempdir().unwrap();
    let rpc = RpcProcess::spawn(
        &python(),
        &args("lsp"),
        temp.path(),
        Framing::ContentLength,
        Dialect::JsonRpc,
        Duration::from_millis(100),
        json!([]),
        json!({}),
    )
    .unwrap();
    assert!(
        rpc.request("fixture/cancelable", json!({}))
            .await
            .unwrap_err()
            .to_string()
            .contains("NEVER auto-retried")
    );
    let state = rpc.request("fixture/state", json!({})).await.unwrap();
    assert_eq!(state["cancelled"].as_array().unwrap().len(), 1);
    assert_eq!(rpc.status().unwrap()["pending_requests"], 0);
    rpc.terminate().await.unwrap();
}

#[tokio::test]
async fn dropping_request_future_cancels_and_backend_remains_usable() {
    let temp = tempfile::tempdir().unwrap();
    let rpc = RpcProcess::spawn(
        &python(),
        &args("lsp"),
        temp.path(),
        Framing::ContentLength,
        Dialect::JsonRpc,
        Duration::from_secs(3),
        json!([]),
        json!({}),
    )
    .unwrap();
    let child_rpc = rpc.clone();
    let request =
        tokio::spawn(async move { child_rpc.request("fixture/cancelable", json!({})).await });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if rpc
            .events()
            .unwrap()
            .iter()
            .any(|e| e["method"] == "fixture/awaiting")
        {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    request.abort();
    let _ = request.await;
    loop {
        let state = rpc.request("fixture/state", json!({})).await.unwrap();
        if state["cancelled"].as_array().unwrap().len() == 1 {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(rpc.status().unwrap()["pending_requests"], 0);
    assert!(!rpc.is_closed());
    rpc.terminate().await.unwrap();
}

#[tokio::test]
async fn backend_progress_and_explicit_restart_are_observable() {
    let temp = tempfile::tempdir().unwrap();
    let rpc = RpcProcess::spawn(
        &python(),
        &args("lsp"),
        temp.path(),
        Framing::ContentLength,
        Dialect::JsonRpc,
        Duration::from_secs(3),
        json!([]),
        json!({}),
    )
    .unwrap();
    rpc.request("fixture/progress", json!({})).await.unwrap();
    assert_eq!(rpc.status().unwrap()["server_status"]["quiescent"], false);
    assert_eq!(
        rpc.status().unwrap()["active_progress"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    rpc.request("fixture/ready", json!({})).await.unwrap();
    assert_eq!(rpc.status().unwrap()["server_status"]["quiescent"], true);
    assert_eq!(
        rpc.status().unwrap()["active_progress"]
            .as_object()
            .unwrap()
            .len(),
        0
    );
    assert!(rpc.request("fixture/exit", json!({})).await.is_err());
    assert!(rpc.is_closed());
    assert!(rpc.request("fixture/state", json!({})).await.is_err());
    let replacement = RpcProcess::spawn(
        &python(),
        &args("lsp"),
        temp.path(),
        Framing::ContentLength,
        Dialect::JsonRpc,
        Duration::from_secs(3),
        json!([]),
        json!({}),
    )
    .unwrap();
    assert!(
        replacement
            .request("fixture/state", json!({}))
            .await
            .is_ok()
    );
    replacement.terminate().await.unwrap();
}

#[tokio::test]
async fn dap_capabilities_and_state_guard_the_complete_session_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let config = CommandConfig {
        program: python(),
        args: args("dap"),
        timeout_secs: 3,
    };
    let adapter = DebugAdapter::start(&config, &SafeRoot::new(temp.path(), &[], 1000).unwrap())
        .await
        .unwrap();
    assert!(
        adapter
            .request("configurationDone", json!({}))
            .await
            .is_err()
    );
    assert!(
        adapter
            .request("evaluate", json!({"expression":"value"}))
            .await
            .is_err()
    );
    assert!(
        adapter
            .request("setFunctionBreakpoints", json!({"breakpoints":[]}))
            .await
            .unwrap_err()
            .to_string()
            .contains("CAPABILITY_UNAVAILABLE")
    );
    adapter
        .request("launch", json!({"program":"fixture"}))
        .await
        .unwrap();
    assert!(adapter.request("attach", json!({})).await.is_err());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while adapter.status().unwrap()["initialized_event"] != true {
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    adapter
        .request("configurationDone", json!({}))
        .await
        .unwrap();
    wait_phase(&adapter, "stopped").await;
    assert!(
        adapter
            .request("configurationDone", json!({}))
            .await
            .is_err()
    );
    adapter
        .request("scopes", json!({"frameId":1}))
        .await
        .unwrap();
    for command in ["next", "stepIn", "stepOut"] {
        adapter
            .request(command, json!({"threadId":1}))
            .await
            .unwrap();
        assert_eq!(
            adapter.status().unwrap()["phase"],
            "stopped",
            "Fast stopped event was lost after {command}"
        );
    }
    adapter
        .request("continue", json!({"threadId":1}))
        .await
        .unwrap();
    wait_phase(&adapter, "running").await;
    assert!(
        adapter
            .request("scopes", json!({"frameId":1}))
            .await
            .is_err()
    );
    adapter
        .request("pause", json!({"threadId":1}))
        .await
        .unwrap();
    wait_phase(&adapter, "stopped").await;
    adapter.request("terminate", json!({})).await.unwrap();
    wait_phase(&adapter, "terminated").await;
    adapter.request("restart", json!({})).await.unwrap();
    wait_phase(&adapter, "running").await;
    adapter.shutdown().await.unwrap();
    assert!(adapter.is_closed());
    assert!(adapter.request("threads", json!({})).await.is_err());
}

#[tokio::test]
async fn dap_cancelled_launch_keeps_an_unknown_outcome_until_disconnect() {
    let temp = tempfile::tempdir().unwrap();
    let config = CommandConfig {
        program: python(),
        args: args("dap"),
        timeout_secs: 3,
    };
    let adapter = DebugAdapter::start(&config, &SafeRoot::new(temp.path(), &[], 1000).unwrap())
        .await
        .unwrap();
    let launch = adapter
        .request("launch", json!({"program":"fixture"}))
        .await
        .unwrap();
    let cancelled = adapter
        .cancel(launch["ticket"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(cancelled["outcome_may_be_unknown"], true);
    assert_eq!(adapter.status().unwrap()["phase"], "outcome_unknown");
    assert!(adapter.request("launch", json!({})).await.is_err());
    assert!(
        adapter.poll().await.unwrap()["pending_tickets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    adapter.shutdown().await.unwrap();
}

#[tokio::test]
async fn dap_restart_ticket_allows_a_new_configuration_cycle_without_deadlock() {
    let temp = tempfile::tempdir().unwrap();
    let mut arguments = args("dap");
    arguments.push("--dap-restart-config".into());
    let config = CommandConfig {
        program: python(),
        args: arguments,
        timeout_secs: 3,
    };
    let adapter = DebugAdapter::start(&config, &SafeRoot::new(temp.path(), &[], 1000).unwrap())
        .await
        .unwrap();
    adapter
        .request("launch", json!({"program":"fixture"}))
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while adapter.status().unwrap()["initialized_event"] != true {
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    adapter
        .request("configurationDone", json!({}))
        .await
        .unwrap();
    wait_phase(&adapter, "stopped").await;
    let restart = adapter.request("restart", json!({})).await.unwrap();
    assert_eq!(restart["pending"], true);
    while adapter.status().unwrap()["configured"] != false {
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    adapter
        .request("configurationDone", json!({}))
        .await
        .unwrap();
    wait_phase(&adapter, "stopped").await;
    assert_eq!(adapter.status().unwrap()["generation"], 2);
    adapter.shutdown().await.unwrap();
}
