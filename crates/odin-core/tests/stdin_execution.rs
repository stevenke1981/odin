//! Real pipe tests with a fixed Rust child, not evidence of a language backend.
use odin_core::{config::CommandConfig, execution::run_with_stdin};
use std::{path::PathBuf, process::Command, sync::OnceLock, time::Duration};

fn executable() -> &'static PathBuf {
    static FIXTURE: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &FIXTURE
        .get_or_init(|| {
            let directory = tempfile::tempdir().unwrap();
            let executable = directory
                .path()
                .join(format!("stdin-workload{}", std::env::consts::EXE_SUFFIX));
            let built = Command::new("rustc")
                .args([
                    "--edition=2024",
                    concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/tests/fixtures/stdin_workload.rs"
                    ),
                    "-o",
                ])
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            (directory, executable)
        })
        .1
}

fn config(mode: &str, timeout_secs: u64) -> CommandConfig {
    CommandConfig {
        program: executable().to_str().unwrap().into(),
        args: vec![mode.into()],
        timeout_secs,
    }
}

#[tokio::test]
async fn unicode_snapshot_roundtrips_exactly_and_empty_input_closes_pipe() {
    let root = tempfile::tempdir().unwrap();
    let input = "測試🦀\r\n\0".repeat(131_072);
    for source in [input.as_str(), ""] {
        let result = run_with_stdin(
            &config("echo", 10),
            root.path(),
            (2_097_152, 1024),
            None,
            source,
        )
        .await
        .unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["stdin_complete"], true);
        assert_eq!(result["stdout_utf8_valid"], true);
        assert_eq!(result["output_truncated"], false);
        assert_eq!(result["stdout"].as_str().unwrap(), source);
        assert_eq!(result["sandbox"], false);
    }
}

#[tokio::test]
async fn concurrent_output_draining_prevents_stdin_backpressure_deadlock() {
    let root = tempfile::tempdir().unwrap();
    let input = "source\n".repeat(131_072);
    let result = run_with_stdin(
        &config("duplex", 10),
        root.path(),
        (1024, 2048),
        None,
        &input,
    )
    .await
    .unwrap();
    assert_eq!(result["success"], true);
    assert_eq!(result["stdin_complete"], true);
    assert_eq!(result["stdout"].as_str().unwrap().len(), 1024);
    assert_eq!(result["stderr"].as_str().unwrap().len(), 2048);
    assert_eq!(result["output_truncated"], true);
}

#[tokio::test]
async fn invalid_utf8_and_nonzero_exit_are_not_usable_source() {
    let root = tempfile::tempdir().unwrap();
    let invalid = run_with_stdin(
        &config("invalid", 10),
        root.path(),
        (1024, 1024),
        None,
        "snapshot",
    )
    .await
    .unwrap();
    assert_eq!(invalid["stdout_utf8_valid"], false);
    assert_eq!(invalid["stderr_utf8_valid"], false);
    let failed = run_with_stdin(&config("exit", 10), root.path(), (1024, 1024), None, "")
        .await
        .unwrap();
    assert_eq!(failed["success"], false);
    assert_eq!(failed["exit_code"], 23);
}

#[tokio::test]
async fn deadline_includes_child_that_does_not_read_stdin() {
    let root = tempfile::tempdir().unwrap();
    let input = "x".repeat(1_048_576);
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        run_with_stdin(
            &config("no-read", 1),
            root.path(),
            (1024, 1024),
            None,
            &input,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result["success"], false);
    assert_eq!(result["timed_out"], true);
    assert_eq!(result["stdin_complete"], false);
}

#[tokio::test]
async fn execution_rejects_unbounded_input_capture_and_deadlines_before_spawn() {
    let root = tempfile::tempdir().unwrap();
    let cfg = config("echo", 10);
    for limits in [(0, 1024), (1024, 0), (16_777_217, 1024), (1024, 4_194_305)] {
        assert_eq!(
            run_with_stdin(&cfg, root.path(), limits, None, "")
                .await
                .unwrap_err()
                .code,
            "INVALID_ARGUMENT"
        );
    }
    let input = "x".repeat(16_777_217);
    assert_eq!(
        run_with_stdin(&cfg, root.path(), (1024, 1024), None, &input)
            .await
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        run_with_stdin(&config("echo", 0), root.path(), (1024, 1024), None, "")
            .await
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
}
