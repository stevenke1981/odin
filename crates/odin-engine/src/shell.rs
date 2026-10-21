use odin_core::{Result, config::CommandConfig, sandbox_policy::MxcSandboxConfig};
use serde_json::Value;
use std::path::Path;

pub async fn run(config: &CommandConfig, root: &Path, max_output: usize) -> Result<Value> {
    odin_core::execution::run(config, root, max_output, None).await
}

pub async fn run_with_sandbox(
    config: &CommandConfig,
    root: &Path,
    max_output: usize,
    sandbox: Option<(&MxcSandboxConfig, &Path)>,
) -> Result<Value> {
    odin_core::execution::run(config, root, max_output, sandbox).await
}
