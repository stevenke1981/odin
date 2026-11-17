use odin_core::{config::Config, sha256};
use odin_engine::Engine;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

const SOURCE: &str = "fn foo() {}\nfn bar() {}\n";

fn setup(
    writable: bool,
    rules: Value,
    peer_args: &[&str],
) -> (tempfile::TempDir, tempfile::TempDir, Arc<Engine>) {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source.rs"), SOURCE).unwrap();
    std::fs::write(root.path().join("other.rs"), "fn other() {}\n").unwrap();
    std::fs::create_dir(root.path().join("vendor")).unwrap();
    std::fs::write(root.path().join("vendor/lib.rs"), "fn lib() {}\n").unwrap();
    let peer = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lsp_peer.py");
    let mut args = vec![peer.to_string_lossy().to_string()];
    args.extend(peer_args.iter().map(|s| s.to_string()));
    let mut project = json!({"root":root.path(),"allow_write":writable,
        "languages":[{"id":"fixture","extensions":["rs"],"program":"python","args":args}]});
    if !rules.is_null() {
        project["refactor_rules"] = rules;
    }
    let config: Config =
        serde_json::from_value(json!({"state_dir":state.path(),"projects":{"one":project}}))
            .unwrap();
    (root, state, Engine::new(config).unwrap())
}
async fn call(engine: &Arc<Engine>, tool: &str, args: Value) -> Value {
    engine.call("one".into(), tool.into(), args).await.unwrap()
}
async fn fail(engine: &Arc<Engine>, tool: &str, args: Value) -> String {
    engine
        .call("one".into(), tool.into(), args)
        .await
        .unwrap_err()
        .code
        .to_string()
}
fn rename_args(new_name: &str) -> Value {
    json!({"relative_path":"source.rs","name_path":"module/foo","new_name":new_name})
}
async fn assert_no_transactions(engine: &Arc<Engine>) {
    let list = call(engine, "transaction_list", json!({})).await;
    assert!(
        list["transactions"].as_array().unwrap().is_empty(),
        "unexpected transactions: {list}"
    );
}
fn full_rules() -> Value {
    json!({"max_files":5,"protected_globs":["vendor/**"],"guidance":["run tests"],
        "naming":[{"kinds":[12],"pattern":"[a-z][a-z0-9_]*","description":"snake_case functions"}]})
}

#[tokio::test]
async fn refactor_rules_tool_reports_configured_rules_on_read_only_project() {
    let (_root, _state, engine) = setup(false, full_rules(), &[]);
    let rules = call(&engine, "refactor_rules", json!({})).await;
    assert_eq!(rules["configured"], true);
    assert_eq!(rules["max_files"], 5);
    assert_eq!(rules["protected_globs"], json!(["vendor/**"]));
    assert_eq!(rules["guidance"], json!(["run tests"]));
    assert_eq!(rules["naming"][0]["pattern"], "[a-z][a-z0-9_]*");
    assert_eq!(rules["naming"][0]["kinds"], json!([12]));
    assert_eq!(rules["violation_code"], "REFACTOR_RULE_VIOLATION");
    assert_eq!(rules["enforced_tools"].as_array().unwrap().len(), 6);
    assert!(!rules["workflow"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn refactor_rules_tool_reports_unconfigured() {
    let (_root, _state, engine) = setup(true, Value::Null, &[]);
    let rules = call(&engine, "refactor_rules", json!({})).await;
    assert_eq!(rules["configured"], false);
    assert_eq!(rules["max_files"], Value::Null);
    assert_eq!(rules["naming"], json!([]));
    assert_eq!(rules["enforced_tools"].as_array().unwrap().len(), 6);
}

#[tokio::test]
async fn rename_violating_naming_rule_is_rejected_before_any_transaction() {
    let (_root, _state, engine) = setup(true, full_rules(), &["--rename"]);
    assert_eq!(
        fail(&engine, "rename_symbol", rename_args("BadName")).await,
        "REFACTOR_RULE_VIOLATION"
    );
    assert_no_transactions(&engine).await;
}

#[tokio::test]
async fn compliant_rename_still_prepares_a_preview() {
    let (root, _state, engine) = setup(true, full_rules(), &["--rename"]);
    let preview = call(&engine, "rename_symbol", rename_args("renamed_fn")).await;
    assert!(preview["transaction_id"].is_string());
    // Proves assert_no_transactions is not vacuous: a prepared id does appear in the list.
    let list = call(&engine, "transaction_list", json!({})).await;
    assert_eq!(list["transactions"].as_array().unwrap().len(), 1, "{list}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
        SOURCE
    );
}

#[tokio::test]
async fn rename_touching_protected_path_or_too_many_files_is_rejected() {
    let (_root, _state, engine) = setup(
        true,
        json!({"protected_globs":["vendor/**"]}),
        &["--rename", "--rename-extra", "vendor/lib.rs"],
    );
    assert_eq!(
        fail(&engine, "rename_symbol", rename_args("renamed_fn")).await,
        "REFACTOR_RULE_VIOLATION"
    );
    assert_no_transactions(&engine).await;
    let (_root, _state, engine) = setup(
        true,
        json!({"max_files":1}),
        &["--rename", "--rename-extra", "other.rs"],
    );
    assert_eq!(
        fail(&engine, "rename_symbol", rename_args("renamed_fn")).await,
        "REFACTOR_RULE_VIOLATION"
    );
    assert_no_transactions(&engine).await;
    let (_root, _state, engine) = setup(
        true,
        json!({"max_files":2}),
        &["--rename", "--rename-extra", "other.rs"],
    );
    let preview = call(&engine, "rename_symbol", rename_args("renamed_fn")).await;
    assert!(preview["transaction_id"].is_string());
}

#[tokio::test]
async fn code_action_refactors_respect_protected_globs_and_max_files() {
    let hash = sha256(SOURCE.as_bytes());
    let move_args = json!({"relative_path":"source.rs","name_path":"module/foo","target_path":"target.rs","action_title":"Move foo","expected_sha256":hash});
    let inline_args = json!({"relative_path":"source.rs","name_path":"module/foo","action_title":"Inline foo","expected_sha256":hash});
    // move_symbol touches source.rs and target.rs.
    let (_root, _state, engine) = setup(true, json!({"max_files":1}), &[]);
    assert_eq!(
        fail(&engine, "move_symbol", move_args.clone()).await,
        "REFACTOR_RULE_VIOLATION"
    );
    assert_no_transactions(&engine).await;
    let (_root, _state, engine) = setup(true, json!({"protected_globs":["target.rs"]}), &[]);
    assert_eq!(
        fail(&engine, "move_symbol", move_args.clone()).await,
        "REFACTOR_RULE_VIOLATION"
    );
    assert_no_transactions(&engine).await;
    // inline_symbol only touches source.rs, so a protected target.rs does not block it.
    let preview = call(&engine, "inline_symbol", inline_args.clone()).await;
    assert!(preview["transaction_id"].is_string());
    let (_root, _state, engine) = setup(true, json!({"protected_globs":["source.rs"]}), &[]);
    assert_eq!(
        fail(&engine, "inline_symbol", inline_args).await,
        "REFACTOR_RULE_VIOLATION"
    );
    // Compliant: max_files 2 allows the two-file move.
    let (_root, _state, engine) = setup(true, json!({"max_files":2}), &[]);
    let preview = call(&engine, "move_symbol", move_args).await;
    assert!(preview["transaction_id"].is_string());
}

#[test]
fn invalid_refactor_rules_in_host_config_fail_to_load() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir(&root).unwrap();
    let path = dir.path().join("odin.toml");
    std::fs::write(&path, "").unwrap();
    let toml = |rules: &str| format!("[projects.demo]\nroot = 'proj'\n{rules}\n");
    let good = toml(
        "[projects.demo.refactor_rules]\nmax_files = 3\n[[projects.demo.refactor_rules.naming]]\npattern = \"[a-z]+\"",
    );
    Config::parse(&good, &path).expect("valid rules load");
    for bad in [
        "[projects.demo.refactor_rules]\n[[projects.demo.refactor_rules.naming]]\npattern = \"(unclosed\"",
        "[projects.demo.refactor_rules]\nmax_files = 0",
        "[projects.demo.refactor_rules]\n[[projects.demo.refactor_rules.naming]]\nkinds = [99]\npattern = \"x\"",
        "[projects.demo.refactor_rules]\nunknown_key = 1",
    ] {
        assert!(
            Config::parse(&toml(bad), &path).is_err(),
            "expected failure for: {bad}"
        );
    }
}

#[tokio::test]
async fn safe_delete_and_quick_fix_actions_respect_protected_globs_and_max_files() {
    let hash = sha256(SOURCE.as_bytes());
    let range = json!({"start":{"line":0,"character":0},"end":{"line":1,"character":0}});
    let delete =
        json!({"relative_path":"source.rs","name_path":"module/foo","expected_sha256":hash});
    let fix = json!({"relative_path":"source.rs","range":range,"action_title":"Fix warning","expected_sha256":hash});
    let resolved = json!({"relative_path":"source.rs","range":range,"action_title":"Resolve fix","expected_sha256":hash});
    let protected = json!({"protected_globs":["source.rs"]});
    let (root, _state, engine) = setup(true, protected.clone(), &["--no-references"]);
    let err = engine
        .call("one".into(), "safe_delete_symbol".into(), delete.clone())
        .await
        .unwrap_err();
    assert_eq!(err.code, "REFACTOR_RULE_VIOLATION", "{}", err.message);
    assert_no_transactions(&engine).await;
    assert_eq!(
        fail(&engine, "apply_quick_fix", fix.clone()).await,
        "REFACTOR_RULE_VIOLATION"
    );
    assert_eq!(
        fail(&engine, "prepare_code_action", resolved.clone()).await,
        "REFACTOR_RULE_VIOLATION"
    );
    assert_no_transactions(&engine).await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.rs")).unwrap(),
        SOURCE
    );
    // max_files = 1 is not exceeded by single-file edits, so these stay compliant.
    let (_root, _state, engine) = setup(true, json!({"max_files":1}), &["--no-references"]);
    for (tool, args) in [
        ("safe_delete_symbol", delete),
        ("apply_quick_fix", fix),
        ("prepare_code_action", resolved),
    ] {
        let preview = call(&engine, tool, args).await;
        assert!(preview["transaction_id"].is_string(), "{tool}");
    }
}
