use odin_core::{config::Config, sha256};
use odin_engine::Engine;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};

const SOURCE: &str = "fn foo() { /* 😀 */ }\r\n";

fn setup(
    writable: bool,
    options: &[String],
) -> (tempfile::TempDir, tempfile::TempDir, Arc<Engine>) {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source.rs"), SOURCE).unwrap();
    let peer = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ide_peer.py");
    let mut arguments = vec![peer.to_string_lossy().to_string()];
    arguments.extend_from_slice(options);
    let config:Config=serde_json::from_value(json!({"state_dir":state.path(),"projects":{"one":{"root":root.path(),"allow_write":writable,
        "languages":[{"id":"ide-fixture","extensions":["rs"],"program":"python","args":arguments}]}}})).unwrap();
    (root, state, Engine::new(config).unwrap())
}
async fn call(engine: &Arc<Engine>, tool: &str, args: Value) -> Value {
    engine.call("one".into(), tool.into(), args).await.unwrap()
}
fn document(args: Value) -> Value {
    let mut args = args;
    args["relative_path"] = json!("source.rs");
    args
}
fn position() -> Value {
    json!({"line":0,"character":2})
}

#[tokio::test]
async fn ide_reads_resolve_metadata_and_never_execute_backend_commands() {
    let (root, _state, engine) = setup(false, &[]);
    let complete = call(
        &engine,
        "completion",
        document(json!({"position":position(),"limit":1})),
    )
    .await;
    assert_eq!(complete["result"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(complete["truncated"], true);
    assert_eq!(complete["document"]["sha256"], sha256(SOURCE.as_bytes()));
    assert_eq!(complete["position_encoding"], "utf-16");
    assert_eq!(complete["freshness"], "current");
    let item = complete["result"]["items"][0].clone();
    let resolved = call(
        &engine,
        "completion_resolve",
        document(json!({"item":item,"expected_sha256":sha256(SOURCE.as_bytes())})),
    )
    .await;
    assert_eq!(resolved["result"]["detail"], "resolved fixture completion");
    assert_eq!(resolved["commands_executed"], false);
    assert_eq!(resolved["source_modified"], false);
    let hover = call(&engine, "hover", document(json!({"position":position()}))).await;
    assert_eq!(hover["result"]["contents"]["value"], "fixture hover");
    let signature = call(
        &engine,
        "signature_help",
        document(json!({"position":position()})),
    )
    .await;
    assert_eq!(
        signature["result"]["signatures"][0]["label"],
        "foo(value: i32)"
    );
    let hints = call(&engine, "inlay_hints", document(json!({"limit":1}))).await;
    assert_eq!(hints["result"].as_array().unwrap().len(), 1);
    assert_eq!(hints["truncated"], true);
    let resolved = call(
        &engine,
        "inlay_hint_resolve",
        document(json!({"hint":hints["result"][0],"expected_sha256":sha256(SOURCE.as_bytes())})),
    )
    .await;
    assert_eq!(resolved["result"]["tooltip"], "resolved fixture hint");
    assert_eq!(resolved["source_modified"], false);
    for args in [
        json!({}),
        json!({"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}}}),
    ] {
        let tokens = call(&engine, "semantic_tokens", document(args)).await;
        assert_eq!(tokens["legend"]["tokenTypes"], json!(["keyword"]));
        assert_eq!(tokens["result"]["data"], json!([0, 0, 2, 0, 1]));
    }
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
        SOURCE
    );
}

#[tokio::test]
async fn workspace_symbols_and_call_hierarchy_are_local_bounded_and_real_rpc() {
    let (_root, _state, engine) = setup(false, &[]);
    let capabilities = call(&engine, "get_capabilities", json!({"probe":true})).await;
    assert_eq!(
        capabilities["languages"][0]["status"]["transport"]["isolation"]["requested"],
        false
    );
    assert_eq!(
        capabilities["languages"][0]["status"]["transport"]["isolation"]["os_boundary_verified"],
        false
    );
    let symbols = call(
        &engine,
        "workspace_symbols",
        json!({"language":"ide-fixture","query":"foo","limit":1,"relative_path":"source.rs"}),
    )
    .await;
    assert_eq!(symbols["result"].as_array().unwrap().len(), 1);
    assert_eq!(symbols["truncated"], true);
    assert_eq!(symbols["total_in_scope"], 2);
    assert_eq!(symbols["excluded_outside_snapshot"], 1);
    assert_eq!(symbols["result"][0]["relative_path"], "source.rs");
    for direction in ["incoming", "outgoing"] {
        let calls = call(
            &engine,
            "call_hierarchy",
            document(json!({"position":position(),"direction":direction})),
        )
        .await;
        let key = if direction == "incoming" {
            "from"
        } else {
            "to"
        };
        assert_eq!(calls["result"][0]["related"][0][key]["name"], "foo");
        assert_eq!(calls["freshness"], "current");
    }
    assert_eq!(
        engine
            .call(
                "one".into(),
                "workspace_symbols".into(),
                json!({"language":"missing","query":"x"})
            )
            .await
            .unwrap_err()
            .code,
        "MISSING_BACKEND"
    );
}

#[tokio::test]
async fn formatting_only_previews_cas_and_revalidates_on_apply() {
    let (root, _state, engine) = setup(true, &[]);
    let preview = call(
        &engine,
        "format_document",
        document(json!({"expected_sha256":sha256(SOURCE.as_bytes()),"options":{"tab_size":2}})),
    )
    .await;
    assert_eq!(preview["source_modified"], false);
    assert_eq!(preview["operation"], "formatting");
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
        SOURCE
    );
    call(
        &engine,
        "transaction_apply",
        json!({"transaction_id":preview["transaction_id"]}),
    )
    .await;
    let formatted = format!("// formatted\n{SOURCE}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
        formatted
    );
    let preview = call(
        &engine,
        "format_document",
        document(json!({"expected_sha256":sha256(formatted.as_bytes())})),
    )
    .await;
    std::fs::write(root.path().join("source.rs"), "external edit\n").unwrap();
    assert_eq!(
        engine
            .call(
                "one".into(),
                "transaction_apply".into(),
                json!({"transaction_id":preview["transaction_id"]})
            )
            .await
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
        "external edit\n"
    );
}

#[tokio::test]
async fn range_formatting_and_empty_edits_keep_explicit_preview_semantics() {
    let (root, _state, engine) = setup(true, &["--empty-formatting".into()]);
    let result = call(
        &engine,
        "format_document",
        document(json!({"expected_sha256":sha256(SOURCE.as_bytes())})),
    )
    .await;
    assert_eq!(result["changed"], false);
    assert_eq!(result["source_modified"], false);
    assert!(result.get("transaction_id").is_none());
    let preview=call(&engine,"format_document",document(json!({"expected_sha256":sha256(SOURCE.as_bytes()),"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}}}))).await;
    assert!(preview["transaction_id"].is_string());
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
        SOURCE
    );
}

#[tokio::test]
async fn ide_policy_hashes_utf16_and_missing_capabilities_fail_closed() {
    let (_root, _state, engine) = setup(false, &["--no-capabilities".into()]);
    let error = engine
        .call(
            "one".into(),
            "hover".into(),
            document(json!({"position":position()})),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "CAPABILITY_UNAVAILABLE");
    assert_eq!(
        engine
            .call(
                "one".into(),
                "format_document".into(),
                document(json!({"expected_sha256":sha256(SOURCE.as_bytes())}))
            )
            .await
            .unwrap_err()
            .code,
        "PERMISSION_DENIED"
    );
    let (_root, _state, engine) = setup(true, &[]);
    assert_eq!(
        engine
            .call(
                "one".into(),
                "completion_resolve".into(),
                document(json!({"item":{"label":"x"},"expected_sha256":"bad"}))
            )
            .await
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    let split = SOURCE[..SOURCE.find('😀').unwrap()].encode_utf16().count() + 1;
    assert!(
        engine
            .call(
                "one".into(),
                "hover".into(),
                document(json!({"position":{"line":0,"character":split}}))
            )
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .call(
                "one".into(),
                "completion".into(),
                document(json!({"position":position(),"caller_argv":["unsafe"]}))
            )
            .await
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
}

#[tokio::test]
async fn source_changed_during_ide_rpc_is_a_conflict_for_reads_and_formatting() {
    for tool in ["hover", "format_document"] {
        let markers = tempfile::tempdir().unwrap();
        let request = markers.path().join("requested");
        let release = markers.path().join("release");
        let (root, _state, engine) = setup(
            true,
            &[
                "--pause".into(),
                "--request-marker".into(),
                request.to_string_lossy().into(),
                "--release-marker".into(),
                release.to_string_lossy().into(),
            ],
        );
        let worker = engine.clone();
        let args = if tool == "hover" {
            document(json!({"position":position()}))
        } else {
            document(json!({"expected_sha256":sha256(SOURCE.as_bytes())}))
        };
        let pending =
            tokio::spawn(async move { worker.call("one".into(), tool.into(), args).await });
        tokio::time::timeout(Duration::from_secs(10), async {
            while !request.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        std::fs::write(
            root.path().join("source.rs"),
            "fn externally_changed() {}\n",
        )
        .unwrap();
        std::fs::write(release, "release").unwrap();
        assert_eq!(pending.await.unwrap().unwrap_err().code, "CONFLICT");
        assert_eq!(
            std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
            "fn externally_changed() {}\n"
        );
    }
}

#[tokio::test]
async fn inspection_catalog_metadata_permissions_and_required_sandbox_are_consistent() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let config:Config=serde_json::from_value(json!({"state_dir":state.path(),"projects":{"one":{"root":root.path(),"allow_inspections":true,"inspections":{"ruff":{"provider":"ruff","command":{"program":"host-secret-executable","args":["check","--no-fix","--no-cache","--output-format=json"]}}}}}})).unwrap();
    let engine = Engine::new(config).unwrap();
    let providers = call(&engine, "list_inspection_providers", json!({})).await;
    assert_eq!(providers["profiles"][0]["requires_sandbox"], true);
    assert_eq!(providers["profiles"][0]["enabled"], false);
    assert_eq!(
        engine
            .call(
                "one".into(),
                "run_inspections".into(),
                json!({"profile":"ruff"})
            )
            .await
            .unwrap_err()
            .code,
        "SANDBOX_REQUIRED"
    );
    let config = call(&engine, "get_current_config", json!({})).await;
    assert!(!config.to_string().contains("host-secret-executable"));
    let capabilities = call(&engine, "get_capabilities", json!({})).await;
    for name in ["run_inspections", "prepare_inspection_fix"] {
        assert_eq!(
            capabilities["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap()["enabled"],
            false
        );
    }
    let (_root, _state, engine) = setup(false, &[]);
    assert_eq!(
        engine
            .call(
                "one".into(),
                "run_inspections".into(),
                json!({"profile":"ruff"})
            )
            .await
            .unwrap_err()
            .code,
        "PERMISSION_DENIED"
    );
}

#[tokio::test]
async fn lsp_coordinates_over_one_hundred_thousand_are_valid_but_utf16_boundaries_still_apply() {
    let (root, _state, engine) = setup(
        false,
        &["--hover-at-request".into(), "--tokens-in-range".into()],
    );
    let source = format!("{}😀x\n", "a".repeat(120000));
    std::fs::write(root.path().join("source.rs"), source.as_bytes()).unwrap();
    let hover = call(
        &engine,
        "hover",
        document(json!({"position":{"line":0,"character":120002}})),
    )
    .await;
    assert_eq!(hover["result"]["range"]["start"]["character"], 120002);
    assert_eq!(hover["document"]["sha256"], sha256(source.as_bytes()));
    for character in [120001, 120004] {
        assert!(
            engine
                .call(
                    "one".into(),
                    "hover".into(),
                    document(json!({"position":{"line":0,"character":character}}))
                )
                .await
                .is_err()
        );
    }
    let source = format!("{}fn\n", "\n".repeat(100001));
    std::fs::write(root.path().join("source.rs"), source.as_bytes()).unwrap();
    let hover = call(
        &engine,
        "hover",
        document(json!({"position":{"line":100001,"character":2}})),
    )
    .await;
    assert_eq!(hover["result"]["range"]["start"]["line"], 100001);
    let range = json!({"start":{"line":100001,"character":0},"end":{"line":100001,"character":2}});
    let tokens = call(&engine, "semantic_tokens", document(json!({"range":range}))).await;
    assert_eq!(tokens["freshness"], "current");
    assert_eq!(
        engine
            .call(
                "one".into(),
                "hover".into(),
                document(json!({"position":{"line":0,"character":2147483648u64}}))
            )
            .await
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        engine.tool("completion").unwrap()["inputSchema"]["properties"]["limit"]["maximum"],
        200
    );
    assert_eq!(
        engine.tool("find_symbol").unwrap()["inputSchema"]["properties"]["offset"]["maximum"],
        100000
    );
}
