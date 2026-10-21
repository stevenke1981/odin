//! Permission/routing tests; no synthetic outcome is claimed as native OS proof.
use odin_core::{config::Config, sha256};
use odin_engine::Engine;
use serde_json::{Value, json};
use std::sync::Arc;

fn setup(write: bool, formatters: bool) -> (tempfile::TempDir, tempfile::TempDir, Arc<Engine>) {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source.py"), "x=1\n").unwrap();
    let config:Config=serde_json::from_value(json!({"state_dir":state.path(),"projects":{"one":{"root":root.path(),"allow_write":write,"allow_formatters":formatters,
        "formatters":{"python":{"provider":"ruff","extensions":["py","pyi"],"command":{"program":"host-private-not-launched-ruff.exe","args":["format","--isolated","--no-cache","--stdin-filename","{relative_path}","-"]}}}}}})).unwrap();
    (root, state, Engine::new(config).unwrap())
}
fn arguments() -> Value {
    json!({"relative_path":"source.py","expected_sha256":sha256(b"x=1\n"),"formatter_profile":"python"})
}
async fn error(engine: &Arc<Engine>, args: Value) -> String {
    engine
        .call("one".into(), "format_document".into(), args)
        .await
        .unwrap_err()
        .code
}

#[tokio::test]
async fn native_formatter_permissions_and_explicit_routing_do_not_enable_a_fallback() {
    for (write, formatter) in [(false, true), (true, false)] {
        let (_root, _state, engine) = setup(write, formatter);
        assert_eq!(error(&engine, arguments()).await, "PERMISSION_DENIED");
    }
    let (root, _state, engine) = setup(true, true);
    assert_eq!(error(&engine, arguments()).await, "SANDBOX_REQUIRED");
    let mut missing = arguments();
    missing["formatter_profile"] = json!("unknown");
    assert_eq!(error(&engine, missing).await, "PERMISSION_DENIED");
    // With no explicit profile, the ordinary LSP route still fails honestly.
    let mut lsp = arguments();
    lsp.as_object_mut().unwrap().remove("formatter_profile");
    assert_eq!(error(&engine, lsp).await, "CAPABILITY_UNAVAILABLE");
    assert_eq!(
        std::fs::read(root.path().join("source.py")).unwrap(),
        b"x=1\n"
    );
}

#[tokio::test]
async fn native_profile_caller_range_options_and_argv_are_never_silently_used() {
    let (_root, _state, engine) = setup(true, true);
    for (key, value) in [
        (
            "range",
            json!({"start":{"line":0,"character":0},"end":{"line":0,"character":1}}),
        ),
        ("options", json!({"tab_size":2})),
        ("options", json!({"insert_spaces":false})),
    ] {
        let mut args = arguments();
        args[key] = value;
        assert_eq!(error(&engine, args).await, "CAPABILITY_UNAVAILABLE");
    }
    let mut args = arguments();
    args["argv"] = json!(["--config", "untrusted.toml"]);
    assert_eq!(error(&engine, args).await, "INVALID_ARGUMENT");
}

#[tokio::test]
async fn formatter_metadata_is_redacted_and_does_not_invent_lsp_capabilities() {
    let (_root, _state, engine) = setup(true, true);
    for tool in ["get_capabilities", "get_current_config"] {
        let result = engine
            .call("one".into(), tool.into(), json!({}))
            .await
            .unwrap();
        assert_eq!(result["formatter_profiles"][0]["name"], "python");
        assert_eq!(result["formatter_profiles"][0]["provider"], "ruff");
        assert_eq!(result["formatter_profiles"][0]["enabled"], false);
        assert_eq!(result["formatter_profiles"][0]["requires_sandbox"], true);
        assert!(!result.to_string().contains("host-private-not-launched"));
        if tool == "get_capabilities" {
            assert_eq!(result["languages"], json!([]));
            assert_eq!(result["semantic_available"], false);
        }
    }
}

#[tokio::test]
async fn native_formatter_rejects_source_writable_sandbox_grants_before_launch() {
    let (root, _state, old_engine) = setup(true, true);
    let scratch = tempfile::tempdir().unwrap();
    let mut config = old_engine.config.clone();
    config.projects.get_mut("one").unwrap().sandbox=Some(serde_json::from_value(json!({"executor":scratch.path().join("not-launched.exe"),"executor_sha256":"0".repeat(64),"temp_root":scratch.path(),"readwrite_paths":[root.path()]})).unwrap());
    drop(old_engine);
    let engine = Engine::new(config).unwrap();
    assert_eq!(error(&engine, arguments()).await, "PERMISSION_DENIED");
    assert_eq!(
        std::fs::read(root.path().join("source.py")).unwrap(),
        b"x=1\n"
    );
}
