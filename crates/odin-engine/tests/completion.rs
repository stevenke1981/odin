use odin_core::{config::Config, sha256};
use odin_engine::Engine;
use serde_json::{Value, json};
use std::sync::Arc;

fn setup(
    writable: bool,
    max_output: usize,
) -> (
    tempfile::TempDir,
    tempfile::TempDir,
    tempfile::TempDir,
    Arc<Engine>,
) {
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let config:Config=serde_json::from_value(json!({"state_dir":state.path(),"limits":{"max_output_bytes":max_output},"allow_global_memory_write":writable,
        "projects":{"one":{"root":one.path(),"allow_write":writable},"two":{"root":two.path(),"allow_write":writable}}})).unwrap();
    let engine = Engine::new(config).unwrap();
    (one, two, state, engine)
}
async fn call(engine: &Arc<Engine>, tool: &str, args: Value) -> Value {
    engine.call("one".into(), tool.into(), args).await.unwrap()
}

#[tokio::test]
async fn version_schema_capabilities_and_policy_agree() {
    let (_one, _two, _state, engine) = setup(false, 131072);
    let version = call(&engine, "get_version", json!({})).await;
    assert_eq!(version["version"], env!("CARGO_PKG_VERSION"));
    let schemas = call(&engine, "get_schemas", json!({"tool":"replace_in_files"})).await;
    assert_eq!(
        schemas["tool"],
        engine.tool("replace_in_files").unwrap().clone()
    );
    let capabilities = call(&engine, "get_capabilities", json!({"probe":true})).await;
    assert_eq!(capabilities["semantic_available"], false);
    assert_eq!(capabilities["languages"], json!([]));
    for name in [
        "memory_import",
        "move_symbol",
        "debug_batch",
        "execute_shell_command",
    ] {
        let spec = capabilities["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap();
        assert_eq!(spec["enabled"], false);
    }
    let error = engine
        .call(
            "one".into(),
            "find_symbol".into(),
            json!({"query":"nothing"}),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "MISSING_BACKEND");
    assert_eq!(
        engine
            .call(
                "one".into(),
                "get_capabilities".into(),
                json!({"language":"missing","probe":true})
            )
            .await
            .unwrap_err()
            .code,
        "MISSING_BACKEND"
    );
}

#[tokio::test]
async fn five_matches_can_select_exactly_two_and_never_write_before_apply() {
    let (one, _two, _state, engine) = setup(true, 131072);
    let before = "foo foo foo foo foo";
    std::fs::write(one.path().join("x"), before).unwrap();
    let mut arguments = json!({"files":[{"relative_path":"x","expected_sha256":sha256(before.as_bytes())}],"needle":"foo","replacement":"BAR","expected_total_matches":5});
    let preview = call(&engine, "replacement_preview", arguments.clone()).await;
    let ids = preview["occurrences"].as_array().unwrap();
    assert_eq!(ids.len(), 5);
    assert_eq!(preview["source_modified"], false);
    arguments["selected_occurrence_ids"] =
        json!([ids[1]["occurrence_id"], ids[3]["occurrence_id"]]);
    let transaction = call(&engine, "replace_in_files", arguments).await;
    assert_eq!(transaction["selected_matches"], 2);
    assert_eq!(
        std::fs::read_to_string(one.path().join("x")).unwrap(),
        before
    );
    call(
        &engine,
        "transaction_apply",
        json!({"transaction_id":transaction["transaction_id"]}),
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(one.path().join("x")).unwrap(),
        "foo BAR foo BAR foo"
    );
}

#[tokio::test]
async fn occurrence_ids_reject_new_snapshots_and_duplicate_targets() {
    let (one, _two, _state, engine) = setup(true, 131072);
    std::fs::write(one.path().join("x"), "foo foo").unwrap();
    let mut arguments = json!({"files":[{"relative_path":"x","expected_sha256":sha256(b"foo foo")}],"needle":"foo","replacement":"bar","expected_total_matches":2});
    let preview = call(&engine, "replacement_preview", arguments.clone()).await;
    arguments["selected_occurrence_ids"] = json!([preview["occurrences"][0]["occurrence_id"]]);
    std::fs::write(one.path().join("x"), "foo foo!").unwrap();
    arguments["files"][0]["expected_sha256"] = json!(sha256(b"foo foo!"));
    assert_eq!(
        engine
            .call("one".into(), "replace_in_files".into(), arguments.clone())
            .await
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    arguments
        .as_object_mut()
        .unwrap()
        .remove("selected_occurrence_ids");
    let duplicate = arguments["files"][0].clone();
    arguments["files"].as_array_mut().unwrap().push(duplicate);
    assert_eq!(
        engine
            .call("one".into(), "replace_in_files".into(), arguments)
            .await
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        std::fs::read_to_string(one.path().join("x")).unwrap(),
        "foo foo!"
    );
}

#[tokio::test]
async fn multiline_regex_selection_preserves_crlf_and_expands_captures() {
    let (one, _two, _state, engine) = setup(true, 131072);
    let text = "前言\r\nfoo1\r\nfoo2\r\n";
    std::fs::write(one.path().join("x"), text).unwrap();
    let mut arguments = json!({"files":[{"relative_path":"x","expected_sha256":sha256(text.as_bytes())}],"needle":"^foo(\\d+)\\r$","replacement":"bar$1\r","regex":true,"multiline":true,"expected_total_matches":2});
    let preview = call(&engine, "replacement_preview", arguments.clone()).await;
    assert_eq!(preview["occurrences"][1]["line"], 3);
    arguments["selected_occurrence_ids"] = json!([preview["occurrences"][1]["occurrence_id"]]);
    let transaction = call(&engine, "replace_in_files", arguments).await;
    call(
        &engine,
        "transaction_apply",
        json!({"transaction_id":transaction["transaction_id"]}),
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(one.path().join("x")).unwrap(),
        "前言\r\nfoo1\r\nbar2\r\n"
    );
}

#[tokio::test]
async fn flow_builtins_run_as_data_and_failed_drafts_do_not_commit() {
    let (_one, _two, _state, engine) = setup(false, 131072);
    let result=call(&engine,"flow_run",json!({"session":"s","steps":[
        {"as":"items","select":[{"keep":true,"value":2},{"keep":false,"value":99},{"keep":true,"value":3}],"filter":{"pointer":"/keep","equals":true}},
        {"as":"total","select":{"$ref":"items"},"map":"/value","reduce":"sum"}
    ]})).await;
    assert_eq!(result["result"], 5.0);
    let error=engine.call("one".into(),"flow_run".into(),json!({"session":"s","steps":[{"as":"uncommitted","select":1},{"as":"missing","select":{"$ref":"absent"}}]})).await.unwrap_err();
    assert_eq!(error.code, "INVALID_ARGUMENT");
    let vars = call(&engine, "flow_vars", json!({"session":"s"})).await;
    assert_eq!(vars["variables"].as_array().unwrap().len(), 2);
    assert_eq!(vars["variables"][0]["name"], "items");
}

#[tokio::test]
async fn memory_bundle_import_and_scope_move_are_previewed_owned_and_guarded() {
    let (_one, _two, _state, engine) = setup(true, 131072);
    call(
        &engine,
        "write_memory",
        json!({"name":"design","content":"Architecture","expected_sha256":null}),
    )
    .await;
    let exported = call(&engine, "memory_export", json!({})).await;
    let preview = engine
        .call(
            "two".into(),
            "memory_import".into(),
            json!({"bundle":exported["bundle"]}),
        )
        .await
        .unwrap();
    assert!(
        engine
            .call("two".into(), "read_memory".into(), json!({"name":"design"}))
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .call(
                "one".into(),
                "memory_transaction_preview".into(),
                json!({"transaction_id":preview["transaction_id"]})
            )
            .await
            .unwrap_err()
            .code,
        "PERMISSION_DENIED"
    );
    engine
        .call(
            "two".into(),
            "memory_transaction_apply".into(),
            json!({"transaction_id":preview["transaction_id"]}),
        )
        .await
        .unwrap();
    let memory = engine
        .call("two".into(), "read_memory".into(), json!({"name":"design"}))
        .await
        .unwrap();
    assert_eq!(memory["content"], "Architecture");
    let moved=call(&engine,"memory_move_scope",json!({"name":"design","destination_project":"one","destination_name":"global/design","expected_sha256":sha256(b"Architecture")})).await;
    call(
        &engine,
        "memory_transaction_apply",
        json!({"transaction_id":moved["transaction_id"]}),
    )
    .await;
    assert!(
        engine
            .call("one".into(), "read_memory".into(), json!({"name":"design"}))
            .await
            .is_err()
    );
    assert_eq!(
        call(&engine, "read_memory", json!({"name":"global/design"})).await["content"],
        "Architecture"
    );
}

#[tokio::test]
async fn results_roundtrip_in_utf8_chunks_with_project_scope_digest_and_expiry() {
    let (one, _two, state, engine) = setup(false, 1024);
    let body = "世界\r\n".repeat(1000);
    std::fs::write(one.path().join("x"), &body).unwrap();
    let spilled = call(&engine, "read_file", json!({"relative_path":"x"})).await;
    assert_eq!(spilled["truncated"], true);
    let id = spilled["result_id"].as_str().unwrap();
    assert_eq!(
        engine
            .call("two".into(), "result_read".into(), json!({"result_id":id}))
            .await
            .unwrap_err()
            .code,
        "RESULT_NOT_FOUND"
    );
    let mut offset = 0u64;
    let mut bytes = Vec::new();
    loop {
        let chunk = call(
            &engine,
            "result_read",
            json!({"result_id":id,"offset":offset,"max_bytes":256}),
        )
        .await;
        let text = chunk["text"].as_str().unwrap();
        assert_eq!(chunk["chunk_sha256"], sha256(text.as_bytes()));
        assert_eq!(chunk["sha256"], spilled["sha256"]);
        bytes.extend_from_slice(text.as_bytes());
        if let Some(next) = chunk["next_offset"].as_u64() {
            offset = next;
        } else {
            break;
        }
    }
    assert_eq!(sha256(&bytes), spilled["sha256"]);
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["content"],
        body
    );
    let metadata = state
        .path()
        .join("projects/one/results")
        .join(format!("{id}.meta.json"));
    let mut value: Value = serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    value["expires_ms"] = json!(0);
    std::fs::write(metadata, serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        engine
            .call("one".into(), "result_read".into(), json!({"result_id":id}))
            .await
            .unwrap_err()
            .code,
        "RESULT_EXPIRED"
    );
}

#[tokio::test]
async fn trace_digest_and_audit_id_match_without_logging_contents() {
    let (_one, _two, _state, engine) = setup(false, 131072);
    let mut version = call(&engine, "get_version", json!({})).await;
    let trace = version.as_object_mut().unwrap().remove("trace").unwrap();
    assert_eq!(
        trace["result_sha256"],
        sha256(&serde_json::to_vec(&version).unwrap())
    );
    let log = call(&engine, "audit_log", json!({"limit":10})).await;
    let entry = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["tool"] == "get_version")
        .unwrap();
    assert_eq!(entry["trace_id"], trace["id"]);
    assert!(entry.get("arguments").is_none());
    assert!(entry.get("result").is_none());
}

#[tokio::test]
async fn incremental_query_sees_new_content_and_deleted_files() {
    let (one, _two, _state, engine) = setup(false, 131072);
    std::fs::write(one.path().join("x"), "initialword").unwrap();
    assert_eq!(
        call(&engine, "index_query", json!({"query":"initialword"})).await["matches"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    std::fs::write(one.path().join("x"), "replacementword").unwrap();
    assert_eq!(
        call(&engine, "index_query", json!({"query":"replacementword"})).await["matches"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    std::fs::remove_file(one.path().join("x")).unwrap();
    assert!(
        call(&engine, "index_query", json!({"query":"replacementword"})).await["matches"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn debug_batch_fails_closed_without_host_debug_permission() {
    let (_one, _two, _state, engine) = setup(false, 131072);
    let error=engine.call("one".into(),"debug_batch".into(),json!({"session_id":"untrusted","commands":[{"command":"evaluate","arguments":{"expression":"anything"}}]})).await.unwrap_err();
    assert_eq!(error.code, "PERMISSION_DENIED");
}

fn setup_lsp() -> (tempfile::TempDir, tempfile::TempDir, Arc<Engine>) {
    assert!(
        std::process::Command::new("python")
            .arg("--version")
            .output()
            .unwrap()
            .status
            .success(),
        "Python fixture peer must actually be installed"
    );
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source.rs"), "fn foo() {}\nfn bar() {}\n").unwrap();
    let peer = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lsp_peer.py");
    let config = serde_json::from_value(
        json!({"state_dir":state.path(),"projects":{"one":{"root":root.path(),"allow_write":true,
        "languages":[{"id":"fixture","extensions":["rs"],"program":"python","args":[peer]}]}}}),
    )
    .unwrap();
    let engine = Engine::new(config).unwrap();
    (root, state, engine)
}

#[tokio::test]
async fn protocol_fixture_symbol_filters_and_capability_probe_use_actual_backend_messages() {
    let (_root, _state, engine) = setup_lsp();
    let capabilities = call(
        &engine,
        "get_capabilities",
        json!({"probe":true,"language":"fixture"}),
    )
    .await;
    assert_eq!(capabilities["languages"][0]["ready"], true);
    assert_eq!(
        capabilities["languages"][0]["capabilities"]["codeActionProvider"]["resolveProvider"],
        true
    );
    let filtered=call(&engine,"find_symbol",json!({"query":"module/*","match_mode":"glob","include_kinds":[12],"depth":1,"group_by_file":true,"limit":1})).await;
    assert_eq!(filtered["symbols"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["symbols"][0]["name_path"], "module/foo");
    assert_eq!(filtered["next_offset"], 1);
    let second=call(&engine,"find_symbol",json!({"query":"module/*","match_mode":"glob","include_kinds":[12],"depth":1,"offset":1,"limit":1})).await;
    assert_eq!(second["symbols"][0]["name_path"], "module/bar");
    assert_eq!(second["next_offset"], Value::Null);
    let overview = call(
        &engine,
        "get_symbols_overview",
        json!({"relative_path":"source.rs","depth":0}),
    )
    .await;
    assert_eq!(overview["symbols"].as_array().unwrap().len(), 1);
    assert_eq!(overview["symbols"][0]["name_path"], "module");
    let declaration = call(
        &engine,
        "find_declaration",
        json!({"relative_path":"source.rs","position":{"line":0,"character":3}}),
    )
    .await;
    assert_eq!(declaration["locations"][0]["external"], false);
    assert_eq!(declaration["locations"][0]["relative_path"], "source.rs");
    assert_eq!(
        declaration["locations"][0]["enclosing_symbol"]["name_path"],
        "module/foo"
    );
    let stopped = call(&engine, "restart_language_server", json!({})).await;
    assert_eq!(stopped["stopped"], true);
    assert_eq!(stopped["shutdown_warnings"], json!([]));
    let snapshot = call(&engine, "get_capabilities", json!({"language":"fixture"})).await;
    assert_eq!(snapshot["languages"][0]["ready"], false);
    assert_eq!(
        call(
            &engine,
            "get_capabilities",
            json!({"language":"fixture","probe":true})
        )
        .await["languages"][0]["ready"],
        true
    );
}

#[tokio::test]
async fn protocol_fixture_move_and_inline_produce_transaction_previews_without_text_fallback() {
    let (root, _state, engine) = setup_lsp();
    let before = "fn foo() {}\nfn bar() {}\n";
    let preview=call(&engine,"move_symbol",json!({"relative_path":"source.rs","name_path":"module/foo","target_path":"target.rs","action_title":"Move foo","expected_sha256":sha256(before.as_bytes())})).await;
    assert_eq!(preview["code_action"]["kind"], "refactor.move");
    assert!(!root.path().join("target.rs").exists());
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
        before
    );
    call(
        &engine,
        "transaction_apply",
        json!({"transaction_id":preview["transaction_id"]}),
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
        "fn bar() {}\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("target.rs")).unwrap(),
        "fn foo() {}\n"
    );
    let (root, _state, engine) = setup_lsp();
    let inline=call(&engine,"inline_symbol",json!({"relative_path":"source.rs","name_path":"module/foo","action_title":"Inline foo","expected_sha256":sha256(before.as_bytes())})).await;
    assert_eq!(inline["code_action"]["kind"], "refactor.inline");
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
        before
    );
}

#[tokio::test]
async fn protocol_fixture_quickfix_uses_diagnostics_and_resolve_but_rejects_server_commands() {
    let (root, _state, engine) = setup_lsp();
    let range = json!({"start":{"line":0,"character":0},"end":{"line":1,"character":0}});
    let hash = sha256(b"fn foo() {}\nfn bar() {}\n");
    let preview=call(&engine,"apply_quick_fix",json!({"relative_path":"source.rs","range":range,"action_title":"Fix warning","expected_sha256":hash})).await;
    assert_eq!(preview["code_action"]["kind"], "quickfix");
    let resolved=call(&engine,"prepare_code_action",json!({"relative_path":"source.rs","range":range,"action_title":"Resolve fix","expected_sha256":hash})).await;
    assert_eq!(resolved["code_action"]["server_commands_executed"], false);
    for (title, code) in [
        ("Unsafe command", "PERMISSION_DENIED"),
        ("Duplicate", "CAPABILITY_UNAVAILABLE"),
        ("Missing action", "CAPABILITY_UNAVAILABLE"),
    ] {
        assert_eq!(engine.call("one".into(),"prepare_code_action".into(),json!({"relative_path":"source.rs","range":range,"action_title":title,"expected_sha256":hash})).await.unwrap_err().code,code);
    }
    std::fs::write(root.path().join("source.rs"), "external write").unwrap();
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
        "external write"
    );
}

#[tokio::test]
async fn protocol_fixture_debug_batch_is_session_bound_and_reports_each_request() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let peer = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dap_peer.py");
    let config = serde_json::from_value(
        json!({"state_dir":state.path(),"projects":{"one":{"root":root.path(),"allow_debug":true,
        "debuggers":{"fixture":{"program":"python","args":[peer]}}}}}),
    )
    .unwrap();
    let engine = Engine::new(config).unwrap();
    let start = call(&engine, "debug_start", json!({"profile":"fixture"})).await;
    let session = start["session_id"].as_str().unwrap();
    assert!(uuid::Uuid::parse_str(session).is_ok());
    assert_eq!(
        engine
            .call(
                "one".into(),
                "debug_batch".into(),
                json!({"session_id":"other","commands":[{"command":"threads","arguments":{}}]})
            )
            .await
            .unwrap_err()
            .code,
        "PERMISSION_DENIED"
    );
    let launch=call(&engine,"debug_batch",json!({"session_id":session,"commands":[
        {"command":"launch","arguments":{"program":"fixture-target"}},{"command":"setBreakpoints","arguments":{"source":{"path":"fixture-target"},"breakpoints":[]}},
        {"command":"configurationDone","arguments":{}}
    ]})).await;
    assert_eq!(launch["success"], true);
    assert_eq!(launch["steps_executed"], 3);
    for outcome in launch["outcomes"].as_array().unwrap() {
        assert!(uuid::Uuid::parse_str(outcome["request_id"].as_str().unwrap()).is_ok());
        assert_eq!(outcome["ok"], true);
    }
    let cancelled=call(&engine,"debug_request",json!({"session_id":session,"command":"cancel_ticket","arguments":{"ticket":launch["outcomes"][0]["result"]["ticket"]}})).await;
    assert_eq!(cancelled["cancelled"], true);
    assert_eq!(cancelled["outcome_may_be_unknown"], true);
    for _ in 0..100 {
        let events = call(&engine, "debug_events", json!({"session_id":session})).await;
        if events["phase"] == "stopped" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let evaluated=call(&engine,"debug_batch",json!({"session_id":session,"commands":[{"command":"evaluate","arguments":{"expression":"6*7","context":"repl"}}]})).await;
    assert_eq!(evaluated["success"], true);
    assert_eq!(evaluated["outcomes"][0]["result"]["result"], "42");
    call(&engine, "debug_stop", json!({"session_id":session})).await;
    assert_eq!(
        engine
            .call(
                "one".into(),
                "debug_batch".into(),
                json!({"session_id":session,"commands":[{"command":"threads","arguments":{}}]})
            )
            .await
            .unwrap_err()
            .code,
        "SESSION_CLOSED"
    );
}

#[tokio::test]
async fn flow_repeated_references_tool_outputs_and_namespace_are_bounded_before_copying() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let config=serde_json::from_value(json!({"state_dir":state.path(),"limits":{"max_session_bytes":1024,"max_file_bytes":4096},"projects":{"one":{"root":root.path()}}})).unwrap();
    let engine = Engine::new(config).unwrap();
    call(
        &engine,
        "flow_run",
        json!({"session":"bounded","steps":[{"as":"base","select":7}]}),
    )
    .await;
    for arguments in [
        json!({"session":"bounded","steps":[{"as":"x","select":"x".repeat(700)}],"return":[{"$ref":"x"},{"$ref":"x"},{"$ref":"x"}]}),
        json!({"session":"bounded","steps":[{"as":"x","select":"x".repeat(700)},{"as":"y","select":{"$ref":"x"}}]}),
        json!({"session":"bounded","steps":[{"as":"x","select":[{"v":"x".repeat(600)},{"v":"y".repeat(600)}],"map":"/v"}]}),
    ] {
        assert_eq!(
            engine
                .call("one".into(), "flow_run".into(), arguments)
                .await
                .unwrap_err()
                .code,
            "LIMIT_EXCEEDED"
        );
        let variables = call(&engine, "flow_vars", json!({"session":"bounded"})).await;
        assert_eq!(variables["variables"].as_array().unwrap().len(), 1);
        assert_eq!(variables["variables"][0]["name"], "base");
    }
    std::fs::write(root.path().join("large"), "x".repeat(1500)).unwrap();
    let request = json!({"session":"bounded","steps":[{"as":"large","tool":"read_file","arguments":{"relative_path":"large"},"pointer":"/sha256"}]});
    assert_eq!(
        engine
            .call("one".into(), "flow_run".into(), request)
            .await
            .unwrap_err()
            .code,
        "LIMIT_EXCEEDED"
    );
}

#[tokio::test]
async fn literal_capture_memory_and_occurrence_growth_fail_before_materializing_output() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let config=serde_json::from_value(json!({"state_dir":state.path(),"limits":{"max_file_bytes":256,"max_output_bytes":2048,"max_matches":8},"projects":{"one":{"root":root.path(),"allow_write":true}}})).unwrap();
    let engine = Engine::new(config).unwrap();
    std::fs::write(root.path().join("x"), "aaaa").unwrap();
    for (is_regex, needle, replacement) in
        [(false, "a", "x".repeat(80)), (true, "(a)", "$1".repeat(80))]
    {
        let arguments = json!({"relative_path":"x","expected_sha256":sha256(b"aaaa"),"needle":needle,"replacement":replacement,"regex":is_regex,"expected_matches":4});
        assert_eq!(
            engine
                .call("one".into(), "replace_content".into(), arguments)
                .await
                .unwrap_err()
                .code,
            "LIMIT_EXCEEDED"
        );
    }
    let arguments = json!({"files":[{"relative_path":"x","expected_sha256":sha256(b"aaaa")}],"needle":"a","replacement":"small","expected_total_matches":4});
    assert_eq!(
        engine
            .call("one".into(), "replacement_preview".into(), arguments)
            .await
            .unwrap_err()
            .code,
        "LIMIT_EXCEEDED"
    );
    call(
        &engine,
        "write_memory",
        json!({"name":"tiny","content":"aaaa","expected_sha256":null}),
    )
    .await;
    assert_eq!(engine.call("one".into(),"edit_memory".into(),json!({"name":"tiny","expected_sha256":sha256(b"aaaa"),"needle":"a","replacement":"x".repeat(80),"expected_matches":4})).await.unwrap_err().code,"LIMIT_EXCEEDED");
    assert_eq!(
        std::fs::read_to_string(root.path().join("x")).unwrap(),
        "aaaa"
    );
    assert_eq!(
        call(&engine, "read_memory", json!({"name":"tiny"})).await["content"],
        "aaaa"
    );
    std::fs::write(root.path().join("many"), "a".repeat(32)).unwrap();
    assert_eq!(engine.call("one".into(),"replace_content".into(),json!({"relative_path":"many","expected_sha256":sha256("a".repeat(32).as_bytes()),"needle":"a","replacement":"x","expected_matches":32})).await.unwrap_err().code,"LIMIT_EXCEEDED");
}

#[tokio::test]
async fn symbol_offset_skips_body_resolution_before_copying() {
    let (_root, _state, old_engine) = setup_lsp();
    let mut config = old_engine.config.clone();
    config.projects.get_mut("one").unwrap().languages[0]
        .args
        .push("--bad-first".into());
    drop(old_engine);
    let engine = Engine::new(config).unwrap();
    let result = call(
        &engine,
        "find_symbol",
        json!({"query":"module/*","match_mode":"glob","offset":1,"limit":1,"include_body":true}),
    )
    .await;
    assert_eq!(result["symbols"].as_array().unwrap().len(), 1);
    assert_eq!(result["symbols"][0]["body"], "fn bar() {}");
}
