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
    client_with_timeout(root, options, Duration::from_secs(3)).await
}
async fn client_with_timeout(
    root: &std::path::Path,
    options: &[&str],
    timeout: Duration,
) -> std::sync::Arc<LspClient> {
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
    LspClient::start(SafeRoot::new(root, &[], 10000).unwrap(), language, timeout)
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
    assert_eq!(
        rpc.status().unwrap()["isolation"]["launch_route"],
        "host_process"
    );
    assert_eq!(
        rpc.status().unwrap()["isolation"]["os_boundary_verified"],
        false
    );
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

fn ide_position() -> odin_protocol::text::Position {
    odin_protocol::text::Position {
        line: 0,
        character: 3,
    }
}
fn ide_range() -> odin_protocol::text::Range {
    odin_protocol::text::Range {
        start: odin_protocol::text::Position {
            line: 0,
            character: 0,
        },
        end: odin_protocol::text::Position {
            line: 0,
            character: 11,
        },
    }
}

#[tokio::test]
async fn ide_reads_and_resolves_are_fresh_inert_and_utf16_negotiated() {
    let temp = tempfile::tempdir().unwrap();
    let before = "fn foo() {}";
    std::fs::write(temp.path().join("lib.rs"), before).unwrap();
    let client = client_with(temp.path(), &["--ide"]).await;
    let hash = odin_core::sha256(before.as_bytes());
    let completion = client.completion("lib.rs", ide_position()).await.unwrap();
    assert_eq!(completion["document"]["sha256"], hash);
    assert_eq!(completion["freshness"], "current");
    let resolved = client
        .resolve_completion("lib.rs", &completion["result"]["items"][0], &hash)
        .await
        .unwrap();
    assert_eq!(resolved["result"]["detail"], "resolved fixture completion");
    assert_eq!(resolved["commands_executed"], false);
    assert_eq!(
        client.hover("lib.rs", ide_position()).await.unwrap()["result"]["contents"]["value"],
        "fixture hover"
    );
    assert_eq!(
        client
            .signature_help("lib.rs", ide_position())
            .await
            .unwrap()["result"]["activeSignature"],
        0
    );
    let hints = client.inlay_hints("lib.rs", ide_range()).await.unwrap();
    let hint = client
        .resolve_inlay_hint("lib.rs", &hints["result"][0], &hash)
        .await
        .unwrap();
    assert_eq!(hint["result"]["tooltip"], "resolved fixture hint");
    assert_eq!(hint["edits_applied"], false);
    for range in [None, Some(ide_range())] {
        let tokens = client.semantic_tokens("lib.rs", range).await.unwrap();
        assert_eq!(tokens["result"]["data"], json!([0, 3, 3, 0, 0]));
        assert_eq!(tokens["legend"]["tokenTypes"][0], "function");
    }
    assert_eq!(
        std::fs::read_to_string(temp.path().join("lib.rs")).unwrap(),
        before
    );
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn ide_workspace_symbols_are_scoped_limited_and_project_local() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("src")).unwrap();
    for path in ["lib.rs", "other.rs", "src/nested.rs"] {
        std::fs::write(temp.path().join(path), "fn foo() {}").unwrap();
    }
    let client = client_with(temp.path(), &["--ide"]).await;
    for path in ["lib.rs", "other.rs", "src/nested.rs"] {
        client.sync(path).await.unwrap();
    }
    let limited = client
        .workspace_symbols_scoped("foo", None, 1)
        .await
        .unwrap();
    assert_eq!(limited["result"].as_array().unwrap().len(), 1);
    assert_eq!(limited["total_in_scope"], 3);
    assert_eq!(limited["truncated"], true);
    assert_eq!(limited["excluded_outside_snapshot"], 1);
    let scoped = client
        .workspace_symbols_scoped("foo", Some("src"), 10)
        .await
        .unwrap();
    assert_eq!(scoped["result"].as_array().unwrap().len(), 1);
    assert_eq!(scoped["result"][0]["relative_path"], "src/nested.rs");
    assert!(
        client
            .workspace_symbols_scoped("foo", Some("../"), 10)
            .await
            .is_err()
    );
    assert!(
        client
            .workspace_symbols_scoped("foo", None, 0)
            .await
            .is_err()
    );
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn ide_call_and_type_hierarchies_preserve_opaque_items_and_ranges() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
    let client = client_with(temp.path(), &["--ide"]).await;
    for direction in ["incoming", "outgoing"] {
        let calls = client
            .call_hierarchy("lib.rs", ide_position(), direction)
            .await
            .unwrap();
        assert_eq!(calls["result"][0]["item"]["data"]["keep"], "opaque");
        assert_eq!(calls["result"][0]["related"].as_array().unwrap().len(), 1);
    }
    for direction in ["super", "sub"] {
        let types = client
            .hierarchy("lib.rs", ide_position(), direction)
            .await
            .unwrap();
        assert_eq!(types["hierarchy"][0]["related"][0]["name"], "foo");
    }
    assert!(
        client
            .call_hierarchy("lib.rs", ide_position(), "unknown")
            .await
            .is_err()
    );
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn ide_formatting_prepares_a_versioned_workspace_edit_for_core_cas_only() {
    use odin_core::transaction::Transactions;
    let temp = tempfile::tempdir().unwrap();
    let before = "fn foo() {}";
    std::fs::write(temp.path().join("lib.rs"), before).unwrap();
    let root = SafeRoot::new(temp.path(), &[], 10000).unwrap();
    let client = client_with(temp.path(), &["--ide"]).await;
    let hash = odin_core::sha256(before.as_bytes());
    let options = json!({"tabSize":4,"insertSpaces":true});
    let edit = client
        .formatting("lib.rs", &options, None, &hash)
        .await
        .unwrap();
    assert!(
        edit["documentChanges"][0]["textDocument"]["version"]
            .as_i64()
            .is_some()
    );
    assert!(
        client
            .formatting("lib.rs", &options, Some(ide_range()), &hash)
            .await
            .is_ok()
    );
    assert_eq!(root.read("lib.rs").unwrap(), before);
    let snapshots = std::collections::BTreeMap::from([("lib.rs".into(), before.into())]);
    let changes = client.changes(&edit, &snapshots).await.unwrap();
    let journal = tempfile::tempdir().unwrap();
    let transactions = Transactions::new(root.clone(), journal.path(), 10).unwrap();
    let preview = transactions.prepare(changes, None).unwrap();
    assert_eq!(root.read("lib.rs").unwrap(), before);
    transactions
        .apply(preview["transaction_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(root.read("lib.rs").unwrap(), "fn formatted() {}");
    assert!(
        client
            .formatting("lib.rs", &options, None, &hash)
            .await
            .unwrap_err()
            .to_string()
            .contains("CONFLICT")
    );
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn ide_queries_reject_stale_resolve_hashes_and_response_time_changes() {
    let temp = tempfile::tempdir().unwrap();
    let before = "fn foo() {}";
    std::fs::write(temp.path().join("lib.rs"), before).unwrap();
    let hash = odin_core::sha256(before.as_bytes());
    let client = client_with(temp.path(), &["--ide"]).await;
    let item =
        client.completion("lib.rs", ide_position()).await.unwrap()["result"]["items"][0].clone();
    let hint = client.inlay_hints("lib.rs", ide_range()).await.unwrap()["result"][0].clone();
    std::fs::write(temp.path().join("lib.rs"), "fn changed() {}").unwrap();
    assert!(
        client
            .resolve_completion("lib.rs", &item, &hash)
            .await
            .unwrap_err()
            .to_string()
            .contains("CONFLICT")
    );
    assert!(
        client
            .resolve_inlay_hint("lib.rs", &hint, &hash)
            .await
            .unwrap_err()
            .to_string()
            .contains("CONFLICT")
    );
    client.shutdown().await.unwrap();
    std::fs::write(temp.path().join("lib.rs"), before).unwrap();
    let racing = client_with(
        temp.path(),
        &["--ide", "--mutate-on", "textDocument/completion"],
    )
    .await;
    assert!(
        racing
            .completion("lib.rs", ide_position())
            .await
            .unwrap_err()
            .to_string()
            .contains("CONFLICT")
    );
    racing.shutdown().await.unwrap();
    std::fs::write(temp.path().join("lib.rs"), before).unwrap();
    let workspace_race =
        client_with(temp.path(), &["--ide", "--mutate-on", "workspace/symbol"]).await;
    workspace_race.sync("lib.rs").await.unwrap();
    assert!(
        workspace_race
            .workspace_symbols("foo")
            .await
            .unwrap_err()
            .to_string()
            .contains("CONFLICT")
    );
    workspace_race.shutdown().await.unwrap();
}

#[tokio::test]
async fn ide_invalid_capabilities_encoding_and_payloads_are_explicit_errors() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
    let unavailable=client_with(temp.path(),&["--ide","--capabilities-json",r#"{"completionProvider":false,"hoverProvider":"true","semanticTokensProvider":{"full":false}}"#]).await;
    assert!(
        unavailable
            .completion("lib.rs", ide_position())
            .await
            .unwrap_err()
            .to_string()
            .contains("CAPABILITY_UNAVAILABLE")
    );
    assert!(
        unavailable
            .hover("lib.rs", ide_position())
            .await
            .unwrap_err()
            .to_string()
            .contains("CAPABILITY_UNAVAILABLE")
    );
    assert!(
        unavailable
            .semantic_tokens("lib.rs", None)
            .await
            .unwrap_err()
            .to_string()
            .contains("CAPABILITY_UNAVAILABLE")
    );
    unavailable.shutdown().await.unwrap();
    let mut arguments = args("lsp");
    arguments.extend(["--encoding".into(), "utf-8".into()]);
    let language = LanguageConfig {
        id: "rust".into(),
        extensions: vec!["rs".into()],
        program: python(),
        args: arguments,
        initialization_options: serde_json::Value::Null,
        settings: serde_json::Value::Null,
    };
    let rejected = LspClient::start(
        SafeRoot::new(temp.path(), &[], 1000).unwrap(),
        language,
        Duration::from_secs(3),
    )
    .await;
    assert!(rejected.err().unwrap().to_string().contains("UTF-16"));
    for malformed in ["tokens", "hierarchy"] {
        let client = client_with(temp.path(), &["--ide", "--ide-malformed", malformed]).await;
        if malformed == "tokens" {
            assert!(client.semantic_tokens("lib.rs", None).await.is_err());
        } else {
            assert!(
                client
                    .call_hierarchy("lib.rs", ide_position(), "incoming")
                    .await
                    .is_err()
            );
        }
        client.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn ide_diagnostics_include_inspection_metadata_and_preserve_backend_failure() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
    let errors = client_with(temp.path(), &["--diagnostic-kind", "errors"]).await;
    let report = errors.diagnostics("lib.rs").await.unwrap();
    assert_eq!(report["status"], "errors");
    assert_eq!(report["summary"]["errors"], 1);
    assert_eq!(report["summary"]["warnings"], 1);
    assert_eq!(report["findings"][0]["rule_id"], "E-FIXTURE");
    assert_eq!(report["findings"][0]["source"], "fixture-linter");
    assert_eq!(report["findings"][0]["relative_path"], "lib.rs");
    assert_eq!(report["summary"]["complete"], true);
    errors.shutdown().await.unwrap();
    for kind in ["backend-error", "malformed"] {
        let broken = client_with(temp.path(), &["--diagnostic-kind", kind]).await;
        assert!(broken.diagnostics("lib.rs").await.is_err());
        broken.shutdown().await.unwrap();
    }
    for (kind, status) in [
        ("none", "pending"),
        ("unversioned", "unverified"),
        ("stale-out-of-bounds", "stale"),
    ] {
        let client = client_with(temp.path(), &["--push-only", "--diagnostic-kind", kind]).await;
        client.symbols("lib.rs").await.unwrap();
        let report = client.diagnostics("lib.rs").await.unwrap();
        assert_eq!(report["status"], status);
        assert_eq!(report["summary"]["complete"], false);
        client.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn sandbox_startup_failure_never_retries_backend_without_sandbox() {
    use odin_core::sandbox_policy::MxcSandboxConfig;
    let temp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let marker = temp.path().join("backend-started.txt");
    let config = MxcSandboxConfig {
        executor: state.path().join("missing-executor.exe"),
        executor_sha256: "0".repeat(64),
        readonly_paths: vec![],
        readwrite_paths: vec![],
        denied_paths: vec![],
        temp_root: state.path().into(),
        allow_ui: false,
    };
    let mut arguments = args("lsp");
    arguments.extend([
        "--started-marker".into(),
        marker.to_string_lossy().into_owned(),
    ]);
    let rpc = RpcProcess::spawn_with_sandbox(
        &python(),
        &arguments,
        temp.path(),
        Framing::ContentLength,
        Dialect::JsonRpc,
        Duration::from_secs(3),
        json!([]),
        json!({}),
        Some((&config, state.path())),
    );
    assert!(rpc.is_err());
    assert!(!marker.exists());
    let language = LanguageConfig {
        id: "rust".into(),
        extensions: vec!["rs".into()],
        program: python(),
        args: arguments,
        initialization_options: serde_json::Value::Null,
        settings: serde_json::Value::Null,
    };
    assert!(
        LspClient::start_with_sandbox(
            SafeRoot::new(temp.path(), &[], 1000).unwrap(),
            language,
            Duration::from_secs(3),
            Some((&config, state.path()))
        )
        .await
        .is_err()
    );
    assert!(!marker.exists());
    let mut arguments = args("dap");
    arguments.extend([
        "--started-marker".into(),
        marker.to_string_lossy().into_owned(),
    ]);
    let command = CommandConfig {
        program: python(),
        args: arguments,
        timeout_secs: 3,
    };
    assert!(
        DebugAdapter::start_with_sandbox(
            &command,
            &SafeRoot::new(temp.path(), &[], 1000).unwrap(),
            Some((&config, state.path()))
        )
        .await
        .is_err()
    );
    assert!(!marker.exists());
}

#[tokio::test]
async fn configured_language_profile_uses_file_specific_lsp_language_id() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("app.tsx"), "fn foo() {}").unwrap();
    let mut arguments = args("lsp");
    arguments.extend([
        "--ide".into(),
        "--expect-language-id".into(),
        "typescriptreact".into(),
    ]);
    let language = LanguageConfig {
        id: "typescript".into(),
        extensions: vec!["tsx".into()],
        program: python(),
        args: arguments,
        initialization_options: serde_json::Value::Null,
        settings: serde_json::Value::Null,
    };
    let client = LspClient::start(
        SafeRoot::new(temp.path(), &[], 1000).unwrap(),
        language,
        Duration::from_secs(3),
    )
    .await
    .unwrap();
    assert!(client.completion("app.tsx", ide_position()).await.is_ok());
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn hierarchy_prepare_quota_rejects_before_any_related_rpc() {
    for calls in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
        let log = temp.path().join("hierarchy.log");
        let log_path = log.to_string_lossy();
        let client = client_with(
            temp.path(),
            &[
                "--ide",
                "--hierarchy-prepare-count",
                "129",
                "--hierarchy-request-log",
                &log_path,
            ],
        )
        .await;
        let error = if calls {
            client
                .call_hierarchy("lib.rs", ide_position(), "incoming")
                .await
        } else {
            client.hierarchy("lib.rs", ide_position(), "super").await
        }
        .unwrap_err();
        assert!(error.to_string().contains("LIMIT_EXCEEDED"), "{error:#}");
        assert!(error.to_string().contains("128 items"), "{error:#}");
        let requests = std::fs::read_to_string(log).unwrap();
        assert_eq!(requests.lines().count(), 1, "{requests}");
        assert!(requests.contains("prepare"));
        client.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn hierarchy_json_quota_is_cumulative_and_stops_before_fifth_related_rpc() {
    for calls in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
        let log = temp.path().join("hierarchy.log");
        let log_path = log.to_string_lossy();
        let client = client_with(
            temp.path(),
            &[
                "--ide",
                "--hierarchy-prepare-count",
                "5",
                "--hierarchy-payload-bytes",
                "1048576",
                "--hierarchy-request-log",
                &log_path,
            ],
        )
        .await;
        let error = if calls {
            client
                .call_hierarchy("lib.rs", ide_position(), "outgoing")
                .await
        } else {
            client.hierarchy("lib.rs", ide_position(), "sub").await
        }
        .unwrap_err();
        assert!(error.to_string().contains("LIMIT_EXCEEDED"), "{error:#}");
        assert!(error.to_string().contains("4194304 bytes"), "{error:#}");
        let requests = std::fs::read_to_string(log).unwrap();
        assert_eq!(requests.lines().count(), 5, "{requests}"); // Prepare + four related.
        client.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn hierarchy_relation_and_range_counts_are_bounded_before_validation() {
    for (option, value, expected) in [
        ("--hierarchy-related-count", "4097", "4096 related items"),
        ("--hierarchy-range-count", "8193", "8192 call ranges"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
        let client = client_with(temp.path(), &["--ide", option, value]).await;
        let error = client
            .call_hierarchy("lib.rs", ide_position(), "incoming")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("LIMIT_EXCEEDED"), "{error:#}");
        assert!(error.to_string().contains(expected), "{error:#}");
        client.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn hierarchy_timeout_covers_prepare_and_all_related_rpcs_together() {
    for calls in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
        let log = temp.path().join("hierarchy.log");
        let log_path = log.to_string_lossy();
        let timeout = Duration::from_millis(800);
        let client = client_with_timeout(
            temp.path(),
            &[
                "--ide",
                "--hierarchy-prepare-count",
                "4",
                "--hierarchy-delay-ms",
                "225", // Each RPC < 800 ms; the five RPCs together > 800 ms.
                "--hierarchy-request-log",
                &log_path,
            ],
            timeout,
        )
        .await;
        let started = std::time::Instant::now();
        let error = if calls {
            client
                .call_hierarchy("lib.rs", ide_position(), "incoming")
                .await
        } else {
            client.hierarchy("lib.rs", ide_position(), "super").await
        }
        .unwrap_err();
        assert!(error.to_string().contains("TIMEOUT"), "{error:#}");
        assert!(error.to_string().contains("total deadline"), "{error:#}");
        assert!(started.elapsed() < timeout + Duration::from_millis(500));
        let requests = std::fs::read_to_string(log).unwrap();
        assert!((2..5).contains(&requests.lines().count()), "{requests}");
        client.shutdown().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "Requires explicitly supplied basedpyright and TypeScript tools; records real diagnostic URI behavior"]
async fn real_push_diagnostic_uri_probe() {
    let tools =
        PathBuf::from(std::env::var("ODIN_URI_PROBE_TOOLS").expect("Set ODIN_URI_PROBE_TOOLS"));
    for language in ["python", "typescript"] {
        let temp = tempfile::tempdir().unwrap();
        let (file, text, program, arguments, initialization) = if language == "python" {
            (
                "sample.py",
                "value: str = 42\n",
                tools
                    .join("python/Scripts/basedpyright-langserver.exe")
                    .to_string_lossy()
                    .into_owned(),
                vec!["--stdio".to_owned()],
                json!(null),
            )
        } else {
            (
                "sample.ts",
                "export const value: string = 42;\n",
                std::env::var("ODIN_NODE").unwrap_or_else(|_| "node".into()),
                vec![
                    "--preserve-symlinks".into(),
                    "--preserve-symlinks-main".into(),
                    tools
                        .join("typescript/node_modules/typescript-language-server/lib/cli.mjs")
                        .to_string_lossy()
                        .into_owned(),
                    "--stdio".into(),
                ],
                json!({"tsserver":{"path":tools.join("typescript/node_modules/typescript/lib/tsserver.js")}}),
            )
        };
        std::fs::write(temp.path().join(file), text).unwrap();
        let workspace_uri = url::Url::from_directory_path(temp.path())
            .unwrap()
            .to_string();
        let open_uri = url::Url::from_file_path(temp.path().join(file))
            .unwrap()
            .to_string();
        let workspace = json!([{"uri":workspace_uri,"name":"uri-probe"}]);
        let rpc = RpcProcess::spawn(&program, &arguments, temp.path(), Framing::ContentLength,
            Dialect::JsonRpc, Duration::from_secs(8), workspace.clone(),
            json!({"basedpyright":{"analysis":{"typeCheckingMode":"basic","diagnosticMode":"openFilesOnly"}},
                "python":{"analysis":{"typeCheckingMode":"basic","diagnosticMode":"openFilesOnly"}}})).unwrap();
        rpc.request("initialize", json!({"processId":std::process::id(),"rootUri":workspace_uri,"workspaceFolders":workspace,
            "initializationOptions":initialization,"capabilities":{"workspace":{"configuration":true,"workspaceFolders":true},
                "textDocument":{"publishDiagnostics":{"versionSupport":true}},"general":{"positionEncodings":["utf-16"]}}})).await.unwrap();
        rpc.notify("initialized", json!({})).await.unwrap();
        rpc.notify(
            "textDocument/didOpen",
            json!({"textDocument":{"uri":open_uri,"languageId":language,"version":1,"text":text}}),
        )
        .await
        .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        let report = loop {
            if let Some(event) = rpc.events().unwrap().into_iter().find(|event| {
                event["method"] == "textDocument/publishDiagnostics"
                    && event["params"]["diagnostics"]
                        .as_array()
                        .is_some_and(|items| !items.is_empty())
            }) {
                break event["params"].clone();
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "No real diagnostic notification: {language}"
            );
            tokio::time::sleep(Duration::from_millis(30)).await;
        };
        eprintln!(
            "URI_PROBE {}",
            json!({"language":language,"did_open_uri":open_uri,
            "publish_diagnostics":report,"cache_lookup":rpc.diagnostics(&open_uri).unwrap().is_some()})
        );
        assert_eq!(rpc.diagnostics(&open_uri).unwrap(), Some(report));
        rpc.terminate().await.unwrap();
    }
}

#[tokio::test]
async fn diagnostic_uri_aliases_share_cache_lookup_and_invalidation() {
    let temp = tempfile::tempdir().unwrap();
    let uri = url::Url::from_file_path(temp.path().join("lib.rs"))
        .unwrap()
        .to_string();
    let mut aliases = vec![uri.replace("/lib.rs", "/%6Cib.rs")];
    if cfg!(windows) {
        let drive = uri.as_bytes()[8].to_ascii_lowercase() as char;
        aliases.push(format!("file:///{drive}%3A{}", &uri[10..]));
        aliases.push(format!("file:///{drive}%3a{}", &uri[10..]));
    }
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
    for alias in &aliases {
        let report = json!({"uri":alias,"version":7,"diagnostics":[]});
        rpc.request("fixture/publishDiagnostics", report.clone())
            .await
            .unwrap();
        assert_eq!(rpc.diagnostics(&uri).unwrap(), Some(report.clone()));
        for lookup in &aliases {
            assert_eq!(rpc.diagnostics(lookup).unwrap(), Some(report.clone()));
        }
        rpc.invalidate_diagnostics(&uri).unwrap();
        assert!(rpc.diagnostics(alias).unwrap().is_none());
        rpc.request(
            "fixture/publishDiagnostics",
            json!({"uri":uri,"version":8,"diagnostics":[]}),
        )
        .await
        .unwrap();
        rpc.invalidate_diagnostics(alias).unwrap();
        assert!(rpc.diagnostics(&uri).unwrap().is_none());
    }
    rpc.terminate().await.unwrap();
}

#[tokio::test]
async fn malformed_diagnostic_uri_is_a_protocol_error_not_a_clean_report() {
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
    let uri = format!(
        "{}?untrusted",
        url::Url::from_file_path(temp.path().join("lib.rs")).unwrap()
    );
    assert!(rpc.diagnostics(&uri).is_err());
    assert!(rpc.invalidate_diagnostics(&uri).is_err());
    let error = rpc
        .request(
            "fixture/publishDiagnostics",
            json!({"uri":uri,"version":1,"diagnostics":[]}),
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("Invalid diagnostic URI"),
        "{error:#}"
    );
    assert!(rpc.is_closed());
    rpc.terminate().await.unwrap();
}

fn replace_fixture_source(
    root: &SafeRoot,
    transactions: &odin_core::transaction::Transactions,
    text: &str,
) {
    let before = root.read("lib.rs").unwrap();
    let preview = transactions
        .prepare(
            vec![odin_core::transaction::Change {
                path: "lib.rs".into(),
                expected_sha256: Some(odin_core::sha256(before.as_bytes())),
                content: Some(text.into()),
            }],
            None,
        )
        .unwrap();
    transactions
        .apply(preview["transaction_id"].as_str().unwrap())
        .unwrap();
}

#[tokio::test]
async fn aliased_push_diagnostics_track_current_errors_clean_recovery_and_source_hash() {
    let mut aliases = vec!["escaped-name"];
    if cfg!(windows) {
        aliases.extend(["drive-colon", "drive-colon-lower"]);
    }
    for alias in aliases {
        let temp = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
        let root = SafeRoot::new(temp.path(), &[], 10000).unwrap();
        let transactions =
            odin_core::transaction::Transactions::new(root.clone(), state.path(), 10).unwrap();
        let client = client_with(
            temp.path(),
            &[
                "--push-only",
                "--diagnostic-kind",
                "source-toggle",
                "--diagnostic-uri-alias",
                alias,
            ],
        )
        .await;
        client.symbols("lib.rs").await.unwrap(); // Response is a barrier after the fixture push.
        let clean = client.diagnostics("lib.rs").await.unwrap();
        assert_eq!(clean["freshness"], "current");
        assert_eq!(clean["status"], "clean");
        assert_eq!(clean["version_verified"], true);
        replace_fixture_source(&root, &transactions, "fn broken() {}");
        client.symbols("lib.rs").await.unwrap();
        let error = client.diagnostics("lib.rs").await.unwrap();
        assert_eq!(error["status"], "errors");
        assert_eq!(error["freshness"], "current");
        assert_eq!(error["version_verified"], true);
        assert_eq!(error["summary"]["errors"], 1);
        assert_eq!(
            error["findings"][0]["source_sha256"],
            odin_core::sha256(b"fn broken() {}")
        );
        assert!(
            error["current_version"].as_i64().unwrap() > clean["current_version"].as_i64().unwrap()
        );
        replace_fixture_source(&root, &transactions, "fn foo() {}");
        client.symbols("lib.rs").await.unwrap();
        let recovered = client.diagnostics("lib.rs").await.unwrap();
        assert_eq!(recovered["status"], "clean");
        assert_eq!(recovered["version_verified"], true);
        assert!(
            recovered["current_version"].as_i64().unwrap()
                > error["current_version"].as_i64().unwrap()
        );
        client.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn aliased_push_diagnostics_preserve_stale_and_unversioned_status() {
    let temp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("lib.rs"), "fn foo() {}").unwrap();
    let root = SafeRoot::new(temp.path(), &[], 10000).unwrap();
    let transactions =
        odin_core::transaction::Transactions::new(root.clone(), state.path(), 10).unwrap();
    let client = client_with(
        temp.path(),
        &[
            "--push-only",
            "--stale-diagnostics",
            "--diagnostic-uri-alias",
            "escaped-name",
        ],
    )
    .await;
    client.symbols("lib.rs").await.unwrap();
    replace_fixture_source(&root, &transactions, "fn broken() {}");
    client.symbols("lib.rs").await.unwrap();
    let stale = client.diagnostics("lib.rs").await.unwrap();
    assert_eq!(stale["freshness"], "stale");
    assert_eq!(stale["version_verified"], false);
    assert_eq!(stale["summary"]["complete"], false);
    client.shutdown().await.unwrap();
    let client = client_with(
        temp.path(),
        &[
            "--push-only",
            "--diagnostic-kind",
            "unversioned",
            "--diagnostic-uri-alias",
            "escaped-name",
        ],
    )
    .await;
    client.symbols("lib.rs").await.unwrap();
    let unverified = client.diagnostics("lib.rs").await.unwrap();
    assert_eq!(unverified["freshness"], "unverified");
    assert_eq!(unverified["version_verified"], false);
    assert_eq!(unverified["summary"]["complete"], false);
    assert!(unverified["findings"][0]["source_sha256"].is_null());
    client.shutdown().await.unwrap();
}
