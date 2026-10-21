use crate::{
    Fault, Result,
    config::CommandConfig,
    process::{ProcessTree, configure_child},
    sandbox::SandboxGuard,
    sandbox_policy::MxcSandboxConfig,
};
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    task::JoinHandle,
};

pub async fn run(
    config: &CommandConfig,
    root: &Path,
    max_output: usize,
    sandbox: Option<(&MxcSandboxConfig, &Path)>,
) -> Result<Value> {
    let (mut command, guard) = if let Some((policy, policy_dir)) = sandbox {
        let launch = crate::sandbox::prepare(
            policy,
            &config.program,
            &config.args,
            root,
            policy_dir,
            Some(Duration::from_secs(config.timeout_secs)),
        )?;
        (Command::from(launch.command), Some(launch.guard))
    } else {
        let mut command = Command::new(&config.program);
        command.args(&config.args).current_dir(root);
        configure_child(command.as_std_mut());
        (command, None)
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let tree = ExecutionLifetime {
        tree: Some(ProcessTree::attach(&child)?),
        guard,
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Fault::new("PROCESS_ERROR", "Missing stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Fault::new("PROCESS_ERROR", "Missing stderr"))?;
    let out = tokio::spawn(capture(stdout, max_output / 2));
    let err = tokio::spawn(capture(stderr, max_output / 2));
    let (status, timed_out) =
        match tokio::time::timeout(Duration::from_secs(config.timeout_secs), child.wait()).await {
            Ok(status) => (Some(status?), false),
            Err(_) => {
                let _ = child.kill().await;
                (None, true)
            }
        };
    drop(tree);
    let stdout = collect(out).await?;
    let stderr = collect(err).await?;
    Ok(
        json!({"program":config.program,"args":config.args,"exit_code":status.as_ref().and_then(|s|s.code()),
        "success":status.as_ref().is_some_and(|s|s.success())&&!timed_out,
        "timed_out":timed_out,"stdout":stdout.0,"stderr":stderr.0,"output_truncated":stdout.1||stderr.1,
        "descendant_process_lifetime_managed":true,"sandbox":sandbox.is_some(),
        "sandbox_status":if sandbox.is_some(){"enforced_mxc_processcontainer"}else{"not_requested"}}),
    )
}
async fn capture<R: AsyncRead + Unpin>(
    mut reader: R,
    cap: usize,
) -> std::io::Result<(String, bool)> {
    let mut collected = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        let keep = n.min(cap.saturating_sub(collected.len()));
        collected.extend_from_slice(&buffer[..keep]);
        if keep < n {
            truncated = true;
        }
    }
    Ok((String::from_utf8_lossy(&collected).into_owned(), truncated))
}
async fn collect(mut task: JoinHandle<std::io::Result<(String, bool)>>) -> Result<(String, bool)> {
    match tokio::time::timeout(Duration::from_secs(2), &mut task).await {
        Ok(Ok(result)) => Ok(result?),
        Ok(Err(error)) => Err(Fault::new("PROCESS_ERROR", error.to_string())),
        Err(_) => {
            task.abort();
            Ok((
                "Output pipe remained open after parent exit; descendant processes may still exist"
                    .into(),
                true,
            ))
        }
    }
}

// Field lifetime ordering matters: terminate descendants before removing the
// kernel profile's private policy and scratch files, including caller cancellation.
struct ExecutionLifetime {
    tree: Option<ProcessTree>,
    guard: Option<SandboxGuard>,
}
impl Drop for ExecutionLifetime {
    fn drop(&mut self) {
        drop(self.tree.take());
        drop(self.guard.take());
    }
}
