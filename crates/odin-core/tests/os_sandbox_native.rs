//! Explicit opt-in native OS isolation checks. No mocks and no Python/Node runtime.
#![cfg(windows)]
use odin_core::{
    process::ProcessTree,
    sandbox::{SandboxLaunch, prepare},
    sandbox_policy::MxcSandboxConfig,
};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, BufReader};

#[test]
fn lifetime_guard_can_be_retained_in_a_shared_rpc_process() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<odin_core::sandbox::SandboxGuard>();
}

struct Fixture {
    _base: TempDir,
    bin: PathBuf,
    read: PathBuf,
    write: PathBuf,
    denied: PathBuf,
    outside: PathBuf,
    policies: PathBuf,
    config: MxcSandboxConfig,
}
impl Fixture {
    fn new() -> Self {
        let base = tempfile::Builder::new()
            .prefix("odin-native-sandbox-")
            .tempdir()
            .unwrap();
        let directories = [
            "bin",
            "read",
            "write",
            "read/private",
            "outside",
            "policies",
            "scratch-root",
        ]
        .map(|name| base.path().join(name));
        for directory in &directories {
            fs::create_dir(directory).unwrap();
        }
        let [bin, read, write, denied, outside, policies, scratch] = directories;
        let built = Command::new("rustc")
            .args([
                "--edition=2024",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/os_sandbox_workload.rs"
                ),
                "-o",
            ])
            .arg(bin.join("workload.exe"))
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        fs::copy(bin.join("workload.exe"), outside.join("workload.exe")).unwrap();
        fs::create_dir(policies.join("child")).unwrap();
        // A junction inside an allowed tree must not grant access to its target.
        let junction = Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(read.join("junction"))
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            junction.status.success(),
            "Local test junction creation failed"
        );
        fs::write(read.join("allowed.txt"), "allowed").unwrap();
        fs::write(denied.join("secret.txt"), "denied").unwrap();
        fs::write(outside.join("outside.txt"), "outside").unwrap();
        let config = MxcSandboxConfig {
            executor: std::env::var_os("ODIN_MXC_EXECUTOR")
                .expect("Explicit ODIN_MXC_EXECUTOR is required")
                .into(),
            executor_sha256: std::env::var("ODIN_MXC_SHA256")
                .expect("Explicit ODIN_MXC_SHA256 is required"),
            readonly_paths: vec![bin.clone(), read.clone()],
            readwrite_paths: vec![write.clone()],
            denied_paths: vec![denied.clone()],
            temp_root: scratch,
            allow_ui: false,
        };
        Self {
            _base: base,
            bin,
            read,
            write,
            denied,
            outside,
            policies,
            config,
        }
    }
    fn launch(&self, args: &[String], timeout: Option<Duration>) -> SandboxLaunch {
        prepare(
            &self.config,
            self.bin.join("workload.exe").to_str().unwrap(),
            args,
            &self.read,
            &self.policies,
            timeout,
        )
        .unwrap()
    }
    fn args(&self, mode: &str, port: u16) -> Vec<String> {
        vec![
            mode.into(),
            self.read.display().to_string(),
            self.write.display().to_string(),
            self.denied.display().to_string(),
            self.outside.display().to_string(),
            port.to_string(),
        ]
    }
}

#[test]
#[ignore = "Requires explicit trusted native MXC path/hash and a supported Windows host"]
fn filesystem_network_stdio_and_descendant_isolation() {
    let fixture = Fixture::new();
    let payload = b"Content-Length: 12\r\n\r\n{\"id\":1}\n\x00\xe6\xb8\xac";
    let SandboxLaunch { mut command, guard } =
        fixture.launch(&["echo".into()], Some(Duration::from_secs(10)));
    let policy_path = PathBuf::from(command.get_args().nth(1).unwrap());
    let policy: Value = serde_json::from_slice(&fs::read(&policy_path).unwrap()).unwrap();
    assert_eq!(policy["fallback"]["allowDaclMutation"], false);
    assert_eq!(policy["process"]["inheritDefaultEnv"], false);
    assert_eq!(policy["network"]["egress"]["default"], "deny");
    assert_eq!(policy["network"]["ingress"]["hostLoopback"], "deny");
    assert_eq!(policy["ui"]["disable"], true);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(payload).unwrap();
    let result = child.wait_with_output().unwrap();
    drop(guard);
    assert!(!policy_path.exists(), "Policy file leaked after child exit");
    assert_eq!(
        fs::read_dir(&fixture.config.temp_root).unwrap().count(),
        0,
        "Private scratch leaked after child exit"
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        result.stdout, payload,
        "Native wrapper must not add framing or diagnostics to stdout"
    );
    assert_eq!(result.stderr, b"fixture-stderr\n");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    listener.accept().unwrap();
    listener.set_nonblocking(true).unwrap();
    for mode in ["check", "child-check"] {
        let SandboxLaunch { mut command, guard } =
            fixture.launch(&fixture.args(mode, port), Some(Duration::from_secs(10)));
        let result = command.stdin(Stdio::null()).output().unwrap();
        drop(guard);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            result.stderr.is_empty(),
            "Unexpected workload diagnostic: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let checks: Value = serde_json::from_slice(&result.stdout).unwrap();
        eprintln!("native_sandbox mode={mode} checks={checks}");
        for field in [
            "read_allowed",
            "write_allowed",
            "readonly_write_denied",
            "denied_read",
            "denied_write",
            "outside_read",
            "outside_write",
            "outside_execute_denied",
            "junction_read_denied",
            "environment_filtered",
            "scratch_write",
            "network_denied",
        ] {
            assert_eq!(checks[field], true, "{mode}: {field}: {checks}");
        }
        assert!(
            matches!(listener.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock),
            "Sandbox established a forbidden host loopback connection"
        );
        assert_eq!(
            fs::read_to_string(fixture.outside.join("outside.txt")).unwrap(),
            "outside"
        );
        assert!(!fixture.outside.join("should-not-exist.txt").exists());
        assert!(!fixture.read.join("should-not-exist.txt").exists());
        assert!(!fixture.denied.join("should-not-exist.txt").exists());
        assert_eq!(
            fs::read_to_string(fixture.read.join("allowed.txt")).unwrap(),
            "allowed"
        );
    }
    let SandboxLaunch { mut command, guard } =
        fixture.launch(&["exit".into(), "7".into()], Some(Duration::from_secs(10)));
    assert_eq!(command.status().unwrap().code(), Some(7));
    drop(guard);
    let mut changed = fixture.config.clone();
    changed.executor_sha256 = "0".repeat(64);
    assert!(
        prepare(
            &changed,
            fixture.bin.join("workload.exe").to_str().unwrap(),
            &[],
            &fixture.read,
            &fixture.policies,
            None
        )
        .is_err()
    );
    changed = fixture.config.clone();
    changed
        .readwrite_paths
        .push(fixture._base.path().to_path_buf());
    assert!(
        prepare(
            &changed,
            fixture.bin.join("workload.exe").to_str().unwrap(),
            &[],
            &fixture.read,
            &fixture.policies,
            None
        )
        .is_err()
    );
    assert!(
        prepare(
            &fixture.config,
            "C:/ODIN-DOES-NOT-EXIST.exe",
            &[],
            &fixture.read,
            &fixture.policies,
            None
        )
        .is_err()
    );
    assert!(!fixture.write.join("failed-launch.txt").exists());
    fs::write(fixture.bin.join("invalid.exe"), b"not a Windows executable").unwrap();
    let SandboxLaunch { mut command, guard } = prepare(
        &fixture.config,
        fixture.bin.join("invalid.exe").to_str().unwrap(),
        &[],
        &fixture.read,
        &fixture.policies,
        Some(Duration::from_secs(2)),
    )
    .unwrap();
    let failure = command.output().unwrap();
    assert!(
        !failure.status.success(),
        "Native launch failure reported success"
    );
    assert!(failure.stdout.is_empty());
    drop(guard);
    eprintln!(
        "native_sandbox launch_failure_exit={:?} no_fallback=true",
        failure.status.code()
    );

    // Preflight rejects case aliases, junction traversal, and broad scratch roots.
    for invalid_case in 0..5 {
        changed = fixture.config.clone();
        match invalid_case {
            0 => changed
                .readonly_paths
                .push(PathBuf::from(fixture.read.to_string_lossy().to_uppercase())),
            1 => changed.readonly_paths.push(fixture.read.join("junction")),
            2 => changed.readwrite_paths.push(fixture.policies.join("child")),
            3 => changed.temp_root = fixture._base.path().to_path_buf(),
            _ => changed.readwrite_paths.push(fixture.read.clone()),
        }
        assert!(
            prepare(
                &changed,
                fixture.bin.join("workload.exe").to_str().unwrap(),
                &[],
                &fixture.read,
                &fixture.policies,
                None
            )
            .is_err(),
            "preflight case {invalid_case}"
        );
    }
}

fn process_exists(pid: u64) -> bool {
    let tasklist =
        PathBuf::from(std::env::var_os("SYSTEMROOT").unwrap()).join("System32/tasklist.exe");
    let output = Command::new(tasklist)
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).contains(&format!(",\"{pid}\","))
}

#[tokio::test]
#[ignore = "Requires explicit trusted native MXC path/hash and a supported Windows host"]
async fn abort_and_native_timeout_reap_the_sandbox_tree() {
    let fixture = Fixture::new();
    for timeout in [None, Some(Duration::from_millis(1500))] {
        let heartbeat = fixture.write.join("heartbeat.txt");
        let _ = fs::remove_file(&heartbeat);
        let args = vec![
            "tree".into(),
            heartbeat.display().to_string(),
            fixture.outside.join("outside.txt").display().to_string(),
        ];
        let SandboxLaunch { command, guard } = fixture.launch(&args, timeout);
        let mut command = tokio::process::Command::from(command);
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let tree = ProcessTree::attach(&child).unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(5), stdout.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        let processes: Value = serde_json::from_str(&line).unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !heartbeat.is_file() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            fs::read_to_string(&heartbeat).unwrap().ends_with(":true"),
            "Descendant escaped filesystem confinement"
        );
        if timeout.is_none() {
            drop(tree);
        } else {
            let status = tokio::time::timeout(Duration::from_secs(8), child.wait())
                .await
                .unwrap()
                .unwrap();
            assert!(
                !status.success(),
                "Timed-out workload must not report success"
            );
            drop(tree);
        }
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        let before = fs::read(&heartbeat).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            fs::read(&heartbeat).unwrap(),
            before,
            "Descendant kept writing after abort/timeout"
        );
        assert!(!process_exists(processes["parent_pid"].as_u64().unwrap()));
        assert!(!process_exists(processes["child_pid"].as_u64().unwrap()));
        eprintln!(
            "native_sandbox lifecycle={timeout:?} parent_and_child_reaped=true heartbeat_stopped=true"
        );
        drop(guard);
    }
}

#[tokio::test]
#[ignore = "Requires explicit trusted native MXC path/hash and a supported Windows host"]
async fn bounded_execution_retains_guard_until_abort_and_timeout_reaping() {
    let fixture = Fixture::new();
    for abort in [true, false] {
        let heartbeat = fixture.write.join("runner-heartbeat.txt");
        let pid_file = fixture.write.join("runner-pids.json");
        let _ = fs::remove_file(&heartbeat);
        let _ = fs::remove_file(&pid_file);
        let config = odin_core::config::CommandConfig {
            program: fixture
                .bin
                .join("workload.exe")
                .to_string_lossy()
                .into_owned(),
            args: vec![
                "tree".into(),
                heartbeat.display().to_string(),
                fixture.outside.join("outside.txt").display().to_string(),
                pid_file.display().to_string(),
            ],
            timeout_secs: 3,
        };
        let policy = fixture.config.clone();
        let policy_dir = fixture.policies.clone();
        let root = fixture.read.clone();
        let task = tokio::spawn(async move {
            odin_core::execution::run(&config, &root, 16_384, Some((&policy, &policy_dir))).await
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !pid_file.is_file() || !heartbeat.is_file() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let processes: Value = serde_json::from_slice(&fs::read(&pid_file).unwrap()).unwrap();
        assert!(
            fs::read_dir(&fixture.config.temp_root)
                .unwrap()
                .next()
                .is_some()
        );
        if abort {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        } else {
            let result = task.await.unwrap().unwrap();
            assert_eq!(result["sandbox"], true);
            assert_eq!(result["success"], false);
            assert!(
                result["timed_out"] == true
                    || result["exit_code"].as_i64().is_some_and(|code| code != 0)
            );
        }
        tokio::time::timeout(Duration::from_secs(8), async {
            while process_exists(processes["parent_pid"].as_u64().unwrap())
                || process_exists(processes["child_pid"].as_u64().unwrap())
                || fs::read_dir(&fixture.config.temp_root)
                    .unwrap()
                    .next()
                    .is_some()
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let bytes = fs::read(&heartbeat).unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(fs::read(&heartbeat).unwrap(), bytes);
        assert!(
            !fs::read_dir(&fixture.policies)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("odin-mxc-policy-"))
        );
        eprintln!(
            "bounded_execution abort={abort} native_parent_child_reaped=true held_scratch_removed=true policy_removed=true"
        );
    }
}
