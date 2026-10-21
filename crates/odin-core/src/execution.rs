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
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    task::JoinHandle,
};

pub async fn run(
    config: &CommandConfig,
    root: &Path,
    max_output: usize,
    sandbox: Option<(&MxcSandboxConfig, &Path)>,
) -> Result<Value> {
    run_capture(
        config,
        root,
        (max_output / 2, max_output / 2),
        sandbox,
        None,
    )
    .await
}

/// Feed one bounded UTF-8 snapshot to a host-configured child. Callers that use
/// stdout as source text must reject truncation and invalid UTF-8 before CAS.
pub async fn run_with_stdin(
    config: &CommandConfig,
    root: &Path,
    output_limits: (usize, usize),
    sandbox: Option<(&MxcSandboxConfig, &Path)>,
    input: &str,
) -> Result<Value> {
    if input.len() > 16 * 1024 * 1024
        || !(1..=16 * 1024 * 1024).contains(&output_limits.0)
        || !(1..=4 * 1024 * 1024).contains(&output_limits.1)
        || !(1..=600).contains(&config.timeout_secs)
    {
        return Err(Fault::invalid(
            "Stdin execution limits outside supported bounds",
        ));
    }
    run_capture(config, root, output_limits, sandbox, Some(input)).await
}

async fn run_capture(
    config: &CommandConfig,
    root: &Path,
    output_limits: (usize, usize),
    sandbox: Option<(&MxcSandboxConfig, &Path)>,
    input: Option<&str>,
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
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut lifetime = ExecutionLifetime {
        child: Some(command.spawn()?),
        tree: None,
        guard,
    };
    lifetime.tree = Some(ProcessTree::attach(lifetime.child()?)?);
    let child = lifetime.child()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Fault::new("PROCESS_ERROR", "Missing stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Fault::new("PROCESS_ERROR", "Missing stderr"))?;
    let out = CaptureTask(tokio::spawn(capture(stdout, output_limits.0)));
    let err = CaptureTask(tokio::spawn(capture(stderr, output_limits.1)));
    let stdin = child.stdin.take();
    let feed = async {
        if let Some(input) = input {
            let mut stdin = stdin.ok_or_else(|| std::io::Error::other("Missing stdin"))?;
            stdin.write_all(input.as_bytes()).await?;
            // Windows Tokio pipes queue blocking writes; shutdown alone does
            // not report a queued write failure. Observe it before success.
            stdin.flush().await?;
            stdin.shutdown().await?;
        }
        Ok::<_, std::io::Error>(())
    };
    // Drain both output pipes while feeding stdin. The same deadline includes
    // a child that never reads stdin, rather than timing only its final wait.
    let (status, timed_out, input_error) =
        match tokio::time::timeout(Duration::from_secs(config.timeout_secs), async {
            tokio::join!(feed, child.wait())
        })
        .await
        {
            Ok((fed, status)) => (
                Some(status?),
                false,
                fed.err().map(|error| error.to_string()),
            ),
            Err(_) => {
                let _ = child.kill().await;
                (None, true, None)
            }
        };
    drop(lifetime);
    let stdout = collect(out).await?;
    let stderr = collect(err).await?;
    Ok(
        json!({"program":config.program,"args":config.args,"exit_code":status.as_ref().and_then(std::process::ExitStatus::code),
        "success":status.as_ref().is_some_and(std::process::ExitStatus::success)&&!timed_out&&input_error.is_none(),
        "timed_out":timed_out,"stdout":stdout.text,"stderr":stderr.text,"output_truncated":stdout.truncated||stderr.truncated,
        "stdout_utf8_valid":stdout.utf8_valid,"stderr_utf8_valid":stderr.utf8_valid,
        "stdin_complete":!timed_out&&input_error.is_none(),"stdin_error":input_error,
        "descendant_process_lifetime_managed":true,"sandbox":sandbox.is_some(),
        "sandbox_status":if sandbox.is_some(){"enforced_mxc_processcontainer"}else{"not_requested"}}),
    )
}
struct Captured {
    text: String,
    truncated: bool,
    utf8_valid: bool,
}

// Dropping a cancelled execution must not detach pipe readers on the runtime.
struct CaptureTask(JoinHandle<std::io::Result<Captured>>);
impl Drop for CaptureTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn capture<R: AsyncRead + Unpin>(mut reader: R, cap: usize) -> std::io::Result<Captured> {
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
    Ok(Captured {
        utf8_valid: std::str::from_utf8(&collected).is_ok(),
        text: String::from_utf8_lossy(&collected).into_owned(),
        truncated,
    })
}
async fn collect(mut task: CaptureTask) -> Result<Captured> {
    match tokio::time::timeout(Duration::from_secs(2), &mut task.0).await {
        Ok(Ok(result)) => Ok(result?),
        Ok(Err(error)) => Err(Fault::new("PROCESS_ERROR", error.to_string())),
        Err(_) => {
            task.0.abort();
            Ok(Captured {
                text: "Output pipe remained open after parent exit; descendant processes may still exist".into(),
                truncated: true,
                utf8_valid: true,
            })
        }
    }
}

// Field lifetime ordering matters: terminate descendants before removing the
// kernel profile's private policy and scratch files, including caller cancellation.
struct ExecutionLifetime {
    child: Option<Child>,
    tree: Option<ProcessTree>,
    guard: Option<SandboxGuard>,
}
impl ExecutionLifetime {
    fn child(&mut self) -> Result<&mut Child> {
        self.child
            .as_mut()
            .ok_or_else(|| Fault::new("PROCESS_ERROR", "Missing child"))
    }
}
impl Drop for ExecutionLifetime {
    fn drop(&mut self) {
        drop(self.tree.take());
        let Some(mut child) = self.child.take() else {
            return;
        };
        let guard = self.guard.take();
        if child.try_wait().is_ok_and(|status| status.is_some()) {
            return;
        }
        let _ = child.start_kill();
        if child.try_wait().is_ok_and(|status| status.is_some()) {
            return;
        }
        // Reap without depending on the caller's Tokio runtime. A cancelled
        // request cannot release its private policy/scratch while MXC is alive.
        std::thread::spawn(move || {
            while !child.try_wait().is_ok_and(|status| status.is_some()) {
                std::thread::sleep(Duration::from_millis(10));
            }
            drop(guard);
        });
    }
}
