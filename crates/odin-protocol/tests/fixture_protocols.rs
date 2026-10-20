//! These tests verify protocol handling against a fixture, NOT real backend parity.
use odin_core::{config::{LanguageConfig,CommandConfig},paths::SafeRoot};
use odin_protocol::{rpc::{RpcProcess,Framing,Dialect},lsp::LspClient,dap::DebugAdapter};
use serde_json::json;
use std::{path::PathBuf,time::Duration};
fn python()->String{std::env::var("ODIN_PYTHON").unwrap_or_else(|_|if cfg!(windows){"python".into()}else{"python3".into()})}
fn args(mode:&str)->Vec<String>{vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/protocol_peer.py").to_string_lossy().into_owned(),mode.into()]}
#[tokio::test]async fn lsp_fixture_lifecycle_and_symbols(){
    let d=tempfile::tempdir().unwrap();std::fs::write(d.path().join("lib.rs"),"fn foo() {}\n").unwrap();
    let language=LanguageConfig{id:"rust".into(),extensions:vec!["rs".into()],program:python(),args:args("lsp"),initialization_options:serde_json::Value::Null,settings:serde_json::Value::Null};
    let client=LspClient::start(SafeRoot::new(d.path(),&[],10000).unwrap(),language,Duration::from_secs(3)).await.unwrap();
    assert_eq!(client.symbols("lib.rs").await.unwrap()[0].name,"foo");
    assert_eq!(client.diagnostics("lib.rs").await.unwrap()["available"],true);
}
#[tokio::test]async fn dap_launch_does_not_block_configuration_done(){
    let d=tempfile::tempdir().unwrap();let config=CommandConfig{program:python(),args:args("dap"),timeout_secs:3};
    let adapter=DebugAdapter::start(&config,&SafeRoot::new(d.path(),&[],1000).unwrap()).await.unwrap();
    assert_eq!(adapter.request("launch",json!({"program":"fixture"})).await.unwrap()["pending"],true);
    adapter.request("configurationDone",json!({})).await.unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    let result=adapter.poll().await.unwrap();assert_eq!(result["launch_results"].as_array().unwrap().len(),1);
}
#[tokio::test]async fn backend_timeout_is_reported_not_retried(){
    let d=tempfile::tempdir().unwrap();
    let rpc=RpcProcess::spawn(&python(),&args("lsp"),d.path(),Framing::ContentLength,Dialect::JsonRpc,Duration::from_millis(100),json!([]),json!({})).unwrap();
    let error=rpc.request("fixture/hang",json!({})).await.unwrap_err();assert!(error.to_string().contains("NEVER auto-retried"));
}
#[tokio::test]
#[ignore="Requires installed rust-analyzer, Rust sysroot, and real fixture project; run explicitly"]
async fn real_rust_analyzer_smoke(){
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/rust-fixture").canonicalize().unwrap();
    let config=LanguageConfig{id:"rust".into(),extensions:vec!["rs".into()],program:"rust-analyzer".into(),args:vec![],initialization_options:serde_json::Value::Null,settings:serde_json::Value::Null};
    let client=LspClient::start(SafeRoot::new(root,&[],100000).unwrap(),config,Duration::from_secs(120)).await.unwrap();
    let symbols=client.symbols("src/lib.rs").await.unwrap();assert!(symbols.iter().any(|s|s.name=="greet"));
}
