//! Re-execute the actual native test binary in two independent OS processes.
use odin_core::{
    paths::SafeRoot,
    sha256,
    transaction::{Change, Transactions},
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn transaction_process_worker() {
    let Some(directory) = std::env::var_os("ODIN_CONTENTION_ROOT") else {
        return;
    };
    let state = std::env::var_os("ODIN_CONTENTION_STATE").unwrap();
    let ready = std::env::var_os("ODIN_CONTENTION_READY").unwrap();
    let token = std::env::var("ODIN_CONTENTION_TOKEN").unwrap();
    let root = SafeRoot::new(&directory, &[], 10000).unwrap();
    let transactions = Transactions::new(root, state, 16).unwrap();
    let prepared = transactions
        .prepare(
            vec![Change {
                path: "source.txt".into(),
                expected_sha256: Some(sha256(b"baseline")),
                content: Some(token),
            }],
            None,
        )
        .unwrap();
    std::fs::write(&ready, b"ready").unwrap();
    let start = Path::new(&directory).join("start");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !start.exists() {
        assert!(
            Instant::now() < deadline,
            "parent never released contention barrier"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    match transactions.apply(prepared["transaction_id"].as_str().unwrap()) {
        Ok(_) => {}
        Err(error) if error.code == "CONFLICT" => std::process::exit(17),
        Err(error) => panic!("unexpected transaction error: {error}"),
    }
}

#[test]
fn two_native_processes_cannot_silently_overwrite_each_other() {
    let directory = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("source.txt"), "baseline").unwrap();
    let executable = std::env::current_exe().unwrap();
    let mut children = Vec::new();
    for token in ["first-writer", "second-writer"] {
        let mut command = Command::new(&executable);
        command
            .args(["--exact", "transaction_process_worker", "--nocapture"])
            .env("ODIN_CONTENTION_ROOT", directory.path())
            .env("ODIN_CONTENTION_STATE", state.path())
            .env("ODIN_CONTENTION_READY", directory.path().join(token))
            .env("ODIN_CONTENTION_TOKEN", token)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        children.push(command.spawn().unwrap());
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while !["first-writer", "second-writer"]
        .iter()
        .all(|token| directory.path().join(token).exists())
    {
        if Instant::now() > deadline {
            for child in &mut children {
                let _ = child.kill();
            }
            panic!("child processes did not reach contention barrier");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::write(directory.path().join("start"), "start").unwrap();
    let mut statuses: Vec<_> = children
        .into_iter()
        .map(|child| child.wait_with_output().unwrap().status.code().unwrap())
        .collect();
    statuses.sort();
    assert_eq!(statuses, vec![0, 17]);
    let content = std::fs::read_to_string(directory.path().join("source.txt")).unwrap();
    assert!(["first-writer", "second-writer"].contains(&content.as_str()));
}
