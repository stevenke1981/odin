use odin_core::config::Config;
use odin_engine::Engine;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

fn setup(restricted: bool) -> (tempfile::TempDir, tempfile::TempDir, PathBuf, Arc<Engine>) {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let log = state.path().join("dap-requests.jsonl");
    let peer = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dap_peer.py");
    let mut project = json!({"root":root.path(),"allow_debug":true,"debuggers":{"fixture":{"program":"python","args":[peer,"--record",log]}}});
    if restricted {
        project["debug_launches"] = json!({"fixture":{"launch":host_payload()}});
    }
    let config: Config =
        serde_json::from_value(json!({"state_dir":state.path(),"projects":{"one":project}}))
            .unwrap();
    (root, state, log, Engine::new(config).unwrap())
}
fn host_payload() -> Value {
    json!({"program":"host-owned-fixture-target","stopOnEntry":true})
}
async fn call(engine: &Arc<Engine>, tool: &str, args: Value) -> Value {
    engine.call("one".into(), tool.into(), args).await.unwrap()
}
async fn records(log: &Path, command: &str) -> Vec<Value> {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let requests = std::fs::read_to_string(log)
                .unwrap_or_default()
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .filter(|request| request["command"] == command)
                .collect::<Vec<_>>();
            if !requests.is_empty() {
                return requests;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture must actually receive the DAP request")
}

#[tokio::test]
async fn immutable_single_launch_uses_host_payload_and_rejects_other_launch_or_missing_attach() {
    for arguments in [json!({}), host_payload()] {
        let (_root, _state, log, engine) = setup(true);
        let start = call(&engine, "debug_start", json!({"profile":"fixture"})).await;
        assert_eq!(start["unrestricted_host_debug"], false);
        for (command, arguments) in [
            ("launch", json!({"program":"caller-target"})),
            ("attach", json!({})),
            ("restart", json!({})),
        ] {
            assert_eq!(engine.call("one".into(),"debug_request".into(),json!({"session_id":start["session_id"],"command":command,"arguments":arguments})).await.unwrap_err().code,"PERMISSION_DENIED");
        }
        let launch = call(
            &engine,
            "debug_request",
            json!({"session_id":start["session_id"],"command":"launch","arguments":arguments}),
        )
        .await;
        assert!(launch["ticket"].is_string());
        let requests = records(&log, "launch").await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["arguments"], host_payload());
        call(
            &engine,
            "debug_stop",
            json!({"session_id":start["session_id"]}),
        )
        .await;
    }
}

#[tokio::test]
async fn batch_launch_policy_is_applied_before_any_adapter_request_and_preserves_per_step_outcomes()
{
    let (_root, _state, log, engine) = setup(true);
    let start = call(&engine, "debug_start", json!({"profile":"fixture"})).await;
    let denied=call(&engine,"debug_batch",json!({"session_id":start["session_id"],"stop_on_error":false,"commands":[
        {"command":"launch","arguments":{"program":"caller-target"}},{"command":"launch","arguments":{}}]})).await;
    assert_eq!(denied["success"], false);
    assert_eq!(denied["steps_executed"], 2);
    assert_eq!(denied["outcomes"][0]["error"]["code"], "PERMISSION_DENIED");
    assert_eq!(denied["outcomes"][1]["ok"], true);
    assert_eq!(denied["unrestricted_host_debug"], false);
    let requests = records(&log, "launch").await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["arguments"], host_payload());
    call(
        &engine,
        "debug_stop",
        json!({"session_id":start["session_id"]}),
    )
    .await;
}

#[tokio::test]
async fn unconfigured_debug_policy_preserves_explicit_privileged_host_behavior_and_reports_it() {
    let (_root, _state, log, engine) = setup(false);
    let start = call(&engine, "debug_start", json!({"profile":"fixture"})).await;
    assert_eq!(start["unrestricted_host_debug"], true);
    let payload = json!({"program":"caller-chosen-fixture-target"});
    call(
        &engine,
        "debug_request",
        json!({"session_id":start["session_id"],"command":"launch","arguments":payload}),
    )
    .await;
    assert_eq!(records(&log, "launch").await[0]["arguments"], payload);
    let events = call(
        &engine,
        "debug_events",
        json!({"session_id":start["session_id"]}),
    )
    .await;
    assert_eq!(events["unrestricted_host_debug"], true);
    call(
        &engine,
        "debug_stop",
        json!({"session_id":start["session_id"]}),
    )
    .await;
}
