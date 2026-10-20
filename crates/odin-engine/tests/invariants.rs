use odin_core::{config::Config,sha256};
use odin_engine::Engine;
use serde_json::json;
use std::sync::Arc;
fn setup(writable:bool)->(tempfile::TempDir,tempfile::TempDir,Arc<Engine>){
    let root=tempfile::tempdir().unwrap();let state=tempfile::tempdir().unwrap();
    let config:Config=serde_json::from_value(json!({"state_dir":state.path(),"projects":{
        "one":{"root":root.path(),"allow_write":writable},"two":{"root":root.path(),"allow_write":false}}})).unwrap();
    let engine=Engine::new(config).unwrap();(root,state,engine)
}
#[tokio::test]async fn readonly_rejects_source_creation(){
    let(_root,_state,e)=setup(false);
    let result=e.call("one".into(),"create_text_file".into(),json!({"relative_path":"x","content":"x","expected_sha256":null})).await.unwrap_err();
    assert_eq!(result.code,"PERMISSION_DENIED");
}
#[tokio::test]async fn exact_hash_required_after_read(){
    let(root,_state,e)=setup(true);std::fs::write(root.path().join("x"),"old").unwrap();
    let result=e.call("one".into(),"replace_content".into(),json!({"relative_path":"x","needle":"old","replacement":"new","expected_sha256":"wrong"})).await.unwrap_err();
    assert_eq!(result.code,"CONFLICT");assert_eq!(std::fs::read_to_string(root.path().join("x")).unwrap(),"old");
}
#[tokio::test]async fn source_edits_are_preview_then_apply(){
    let(root,_state,e)=setup(true);
    let preview=e.call("one".into(),"create_text_file".into(),json!({"relative_path":"hello","content":"世界","expected_sha256":null})).await.unwrap();
    assert!(!root.path().join("hello").exists());
    e.call("one".into(),"transaction_apply".into(),json!({"transaction_id":preview["transaction_id"]})).await.unwrap();
    assert_eq!(std::fs::read_to_string(root.path().join("hello")).unwrap(),"世界");
}
#[tokio::test]async fn project_override_is_rejected(){
    let(_r,_s,e)=setup(true);let error=e.call("one".into(),"status".into(),json!({"project":"two"})).await.unwrap_err();assert_eq!(error.code,"PERMISSION_DENIED");
}
#[tokio::test]async fn flow_cannot_reach_mutating_tools(){
    let(root,_s,e)=setup(true);let error=e.call("one".into(),"flow_run".into(),json!({"session":"s","steps":[{"as":"bad","tool":"create_text_file","arguments":{"relative_path":"x","content":"x","expected_sha256":null}}]})).await.unwrap_err();
    assert_eq!(error.code,"PERMISSION_DENIED");assert!(!root.path().join("x").exists());
}
#[tokio::test]async fn flow_state_is_scoped_to_project(){
    let(_r,_s,e)=setup(true);
    e.call("one".into(),"flow_run".into(),json!({"session":"same","steps":[{"as":"a","select":[1,2,3]}]})).await.unwrap();
    let vars=e.call("two".into(),"flow_vars".into(),json!({"session":"same"})).await.unwrap();assert!(vars["variables"].as_array().unwrap().is_empty());
}
#[tokio::test]async fn cross_project_queries_are_read_only(){
    let(_r,_s,e)=setup(true);let err=e.call("one".into(),"query_project".into(),json!({"target_project":"one","tool":"transaction_apply","arguments":{"transaction_id":"ignored"}})).await.unwrap_err();
    assert_eq!(err.code,"PERMISSION_DENIED");
}
#[tokio::test]async fn removed_bridge_tools_are_unknown(){
    let(_r,_s,e)=setup(true);
    for name in ["serena_tools", "serena_call"] {
        assert!(e.tool(name).is_none());
        let err=e.call("one".into(),name.into(),json!({})).await.unwrap_err();
        assert_eq!(err.code,"UNKNOWN_TOOL");
    }
}
#[tokio::test]async fn standalone_status_and_catalog_agree(){
    let(_r,_s,e)=setup(false);
    let status=e.call("one".into(),"status".into(),json!({})).await.unwrap();
    assert_eq!(status["standalone"],true);
    assert_eq!(status["external_tool_bridge"],false);
    assert_eq!(status["tool_count"],58);
    assert!(e.tools().iter().all(|tool| tool["policy"]!="bridge"));
}
#[test]fn legacy_bridge_configuration_is_rejected(){
    let config=serde_json::from_value::<Config>(json!({"projects":{
        "one":{"root":".","serena":{"program":"serena","args":[],"trusted_execution":true}}
    }}));
    assert!(config.is_err(),"Removed privileged configuration must not be silently ignored");
}
#[tokio::test]async fn malformed_arguments_do_not_fall_back_to_defaults(){
    let(_r,_s,e)=setup(true);let err=e.call("one".into(),"read_file".into(),json!({"relative_path":"x","start_line":"1"})).await.unwrap_err();assert_eq!(err.code,"INVALID_ARGUMENT");
}
#[tokio::test]async fn memory_roundtrip_has_hash(){
    let(_r,_s,e)=setup(true);e.call("one".into(),"write_memory".into(),json!({"name":"architecture","content":"Rust","expected_sha256":null})).await.unwrap();
    let result=e.call("one".into(),"read_memory".into(),json!({"name":"architecture"})).await.unwrap();assert_eq!(result["sha256"],sha256(b"Rust"));
}
