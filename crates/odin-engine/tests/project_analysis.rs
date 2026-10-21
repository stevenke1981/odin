use odin_core::config::Config;
use odin_engine::Engine;
use serde_json::{Value, json};

#[tokio::test]
async fn live_analysis_updates_structure_and_evidence_without_running_backends() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname='first'").unwrap();
    std::fs::write(root.path().join("main.rs"), "fn first() {}").unwrap();
    std::fs::write(root.path().join(".env"), "SECRET").unwrap();
    let config: Config = serde_json::from_value(json!({"state_dir":state.path(),"projects":{"current":{"root":root.path(),
        "languages":[{"id":"rust","extensions":["rs"],"program":"must-never-start-for-structural-analysis"}]}}})).unwrap();
    let e = Engine::new(config).unwrap();
    let first = e
        .call("current".into(), "project_analysis".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(first["file_count"], 2);
    assert_eq!(first["tests"], "NOT_RUN");
    assert_eq!(first["backends_started"], false);
    assert_eq!(first["permissions"]["write"], false);
    assert_eq!(first["language_hints"][0]["id"], "rust");
    assert_eq!(first["language_hints"][0]["host_backend_configured"], true);
    let evidence_hash = first["evidence"][0]["sha256"].clone();
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname='changed'").unwrap();
    std::fs::write(root.path().join("new.py"), "print('new')").unwrap();
    std::fs::remove_file(root.path().join("main.rs")).unwrap();
    let second = e
        .call("current".into(), "project_analysis".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(second["extensions"]["rs"], Value::Null);
    assert_eq!(second["extensions"]["py"], 1);
    assert_ne!(evidence_hash, second["evidence"][0]["sha256"]);
    assert_ne!(first["structure_sha256"], second["structure_sha256"]);
    assert!(
        second["unconfigured_language_hints"]
            .as_array()
            .unwrap()
            .contains(&json!("python"))
    );
    let onboarding = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        e.call("current".into(), "onboarding".into(), json!({})),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(onboarding["analysis"]["extensions"]["py"], 1);
    assert!(!root.path().join("executed").exists());
    let error = e
        .call(
            "current".into(),
            "project_analysis".into(),
            json!({"root":"/arbitrary"}),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "INVALID_ARGUMENT");
}

#[tokio::test]
async fn analysis_reports_scan_evidence_limits_and_unreadable_markers() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), [0xff, 0xfe]).unwrap();
    std::fs::write(
        root.path().join("package.json"),
        "{\"scripts\":{\"test\":\"never execute\"}}",
    )
    .unwrap();
    let config: Config = serde_json::from_value(json!({"state_dir":state.path(),"limits":{"max_files":1},"projects":{"p":{"root":root.path()}}})).unwrap();
    let e = Engine::new(config).unwrap();
    let result = e
        .call(
            "p".into(),
            "project_analysis".into(),
            json!({"sample_files":0,"max_evidence":1}),
        )
        .await
        .unwrap();
    assert_eq!(result["scan_truncated"], true);
    assert!(result["sample_files"].as_array().unwrap().is_empty());
    // Use an unrestricted second engine to ensure both markers are inspected.
    let mut config = e.config.clone();
    config.limits.max_files = 100;
    let all = Engine::new(config).unwrap();
    let result = all
        .call("p".into(), "project_analysis".into(), json!({}))
        .await
        .unwrap();
    let unreadable = result["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == "Cargo.toml")
        .unwrap();
    assert_eq!(unreadable["status"], "UNREADABLE");
    assert!(unreadable["sha256"].is_null());
    let bounded = all
        .call(
            "p".into(),
            "project_analysis".into(),
            json!({"max_evidence":1}),
        )
        .await
        .unwrap();
    assert_eq!(bounded["evidence_truncated"], true);
}

#[tokio::test]
async fn workspace_stores_and_memory_namespaces_are_isolated() {
    let base = tempfile::tempdir().unwrap();
    let a = base.path().join("a");
    let b = base.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let host = base.path().join("host.toml");
    let config: Config = serde_json::from_value(
        json!({"state_dir":base.path().join("state"),"workspace":{"allowed_roots":[base.path()]}}),
    )
    .unwrap();
    std::fs::write(
        &host,
        format!(
            "state_dir = {}\n[workspace]\nallowed_roots = [{}]\n",
            json!(config.state_dir),
            json!(base.path())
        ),
    )
    .unwrap();
    let first = Engine::new(Config::load_with_workspace(&host, Some(&a)).unwrap()).unwrap();
    let second = Engine::new(Config::load_with_workspace(&host, Some(&b)).unwrap()).unwrap();
    let id_a = first.default_project().unwrap();
    let id_b = second.default_project().unwrap();
    assert_ne!(id_a, id_b);
    assert!(
        base.path()
            .join("state/projects")
            .join(&id_a)
            .join("index.sqlite3")
            .is_file()
    );
    assert!(
        base.path()
            .join("state/projects")
            .join(&id_b)
            .join("index.sqlite3")
            .is_file()
    );
    assert!(
        second
            .call(id_a, "list_memories".into(), json!({}))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn language_profile_id_and_partial_extensions_do_not_hide_backend_gaps() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("view.tsx"), "export const view = 1;").unwrap();
    let config: Config = serde_json::from_value(
        json!({"state_dir":state.path(),"projects":{"p":{"root":root.path(),
        "languages":[{"id":"typescript","extensions":["ts"],"program":"never-start"}]}}}),
    )
    .unwrap();
    let e = Engine::new(config).unwrap();
    let report = e
        .call("p".into(), "project_analysis".into(), json!({}))
        .await
        .unwrap();
    let language = report["language_hints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "typescript")
        .unwrap();
    assert_eq!(language["host_backend_configured"], false);
    assert_eq!(language["unconfigured_extensions"], json!(["tsx"]));
    assert!(
        report["unconfigured_language_hints"]
            .as_array()
            .unwrap()
            .contains(&json!("typescript"))
    );
    assert_eq!(
        e.call(
            "p".into(),
            "get_symbols_overview".into(),
            json!({"relative_path":"view.tsx"})
        )
        .await
        .unwrap_err()
        .code,
        "CAPABILITY_UNAVAILABLE"
    );
}
