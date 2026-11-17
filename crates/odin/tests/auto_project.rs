//! Real-binary tests for `project add`, `setup` selection and session default projects.
//! Each test runs the compiled `odin` binary against a temporary host config.
use odin_core::config::Config;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"odin-test","version":"0"}}}"#;
const TOKEN: &str = "odin-test-token-0123456789abcdefghijklmnop";

struct Fixture {
    dir: PathBuf,
    config: PathBuf,
    app: PathBuf,
}

fn workspace(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("auto-project-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn quoted(path: &Path) -> String {
    serde_json::to_string(&path.display().to_string()).unwrap()
}

fn fixture(name: &str) -> Fixture {
    let dir = workspace(name);
    let app = dir.join("app");
    let other = dir.join("other");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    let config = dir.join("odin.toml");
    let text = format!(
        "state_dir = {}\n\n[projects.app]\nroot = {}\nallow_write = true\n\n[projects.other]\nroot = {}\n",
        quoted(&dir.join("state")),
        quoted(&app),
        quoted(&other)
    );
    std::fs::write(&config, text).unwrap();
    Fixture { dir, config, app }
}

fn odin(config: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_odin"));
    command.arg("--config").arg(config);
    command
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not JSON ({error}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn sha256_of(path: &Path) -> String {
    odin_core::sha256(&std::fs::read(path).unwrap())
}

fn project_add(config: &Path, args: &[&str]) -> Output {
    odin(config)
        .args(["project", "add"])
        .args(args)
        .output()
        .unwrap()
}

/// Spawns `serve --transport stdio` from `cwd`, performs initialize, and returns the
/// JSON-RPC response plus the stderr text captured after the child is stopped.
fn initialize_stdio(config: &Path, args: &[&str], cwd: &Path) -> (Value, String) {
    let mut child = odin(config)
        .args(["serve", "--transport", "stdio"])
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{INITIALIZE}").unwrap();
    stdin.flush().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = receiver
        .recv_timeout(Duration::from_secs(60))
        .expect("initialize response within 60 seconds");
    drop(stdin);
    let _ = child.kill();
    let output = child.wait_with_output().unwrap();
    let value = serde_json::from_str(&line).expect("initialize response is JSON");
    (value, stderr_text(&output))
}

fn instructions(response: &Value) -> String {
    response["result"]["instructions"]
        .as_str()
        .expect("initialize result carries instructions")
        .to_owned()
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Minimal HTTP/1.1 POST to `/mcp` with the bearer token and loopback Host header.
fn post_initialize(port: u16) -> Value {
    let body = INITIALIZE;
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\n\
         Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).into_owned();
    assert!(
        text.starts_with("HTTP/1.1 200"),
        "unexpected HTTP response: {text}"
    );
    let payload = text
        .lines()
        .find_map(|line| {
            let value = line.trim();
            value
                .strip_prefix("data:")
                .map(str::trim)
                .filter(|json| json.starts_with('{'))
        })
        .unwrap_or_else(|| text.split("\r\n\r\n").last().unwrap_or("").trim());
    serde_json::from_str(payload).unwrap_or_else(|error| panic!("{error}: {text}"))
}

#[test]
fn project_add_dry_run_leaves_host_file_unchanged() {
    let fixture = fixture("dry-run");
    let new_root = fixture.dir.join("newproj");
    std::fs::create_dir_all(&new_root).unwrap();
    let before = std::fs::read(&fixture.config).unwrap();
    let output = project_add(&fixture.config, &["--root", new_root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr_text(&output));
    let value = stdout_json(&output);
    assert_eq!(value["operation"], "DRY_RUN");
    assert_eq!(value["project"], "newproj");
    assert_eq!(value["expected_sha256"], sha256_of(&fixture.config));
    assert_eq!(value["host_permissions_changed"], true);
    assert_eq!(value["allow_write"], false);
    assert!(value.get("client_connection").is_none());
    assert!(
        value["note"]
            .as_str()
            .unwrap()
            .contains("[projects.newproj]")
    );
    assert_eq!(std::fs::read(&fixture.config).unwrap(), before);
}

#[test]
fn project_add_apply_with_expected_hash_adds_project_and_keeps_backup() {
    let fixture = fixture("apply");
    let new_root = fixture.dir.join("newproj");
    std::fs::create_dir_all(&new_root).unwrap();
    let before = std::fs::read(&fixture.config).unwrap();
    let hash = sha256_of(&fixture.config);
    let output = project_add(
        &fixture.config,
        &[
            "--root",
            new_root.to_str().unwrap(),
            "--name",
            "Second-App",
            "--apply",
            "--expected-sha256",
            &hash,
        ],
    );
    assert!(output.status.success(), "{}", stderr_text(&output));
    let value = stdout_json(&output);
    assert_eq!(value["operation"], "APPLIED");
    assert_eq!(value["project"], "Second-App");
    assert_eq!(value["host_permissions_changed"], true);
    assert_eq!(value["allow_write"], false);
    assert!(value.get("client_connection").is_none());
    assert!(
        value["note"]
            .as_str()
            .unwrap()
            .contains("Restart Odin servers")
    );
    let backup = PathBuf::from(value["backup"].as_str().expect("backup path"));
    assert_eq!(std::fs::read(&backup).unwrap(), before);
    let config = Config::load(&fixture.config).unwrap();
    let project = &config.projects["Second-App"];
    assert_eq!(project.root, new_root.canonicalize().unwrap());
    assert!(!project.allow_write);
    assert!(!project.allow_shell);
    assert!(!project.allow_debug);
    assert!(project.commands.is_empty());
    assert!(project.languages.is_empty());
    assert!(config.projects.contains_key("app"));
}

#[test]
fn project_add_writable_flag_sets_allow_write_only() {
    let fixture = fixture("writable");
    let new_root = fixture.dir.join("writable-root");
    std::fs::create_dir_all(&new_root).unwrap();
    let hash = sha256_of(&fixture.config);
    let output = project_add(
        &fixture.config,
        &[
            "--root",
            new_root.to_str().unwrap(),
            "--name",
            "writable-root",
            "--writable",
            "--apply",
            "--expected-sha256",
            &hash,
        ],
    );
    assert!(output.status.success(), "{}", stderr_text(&output));
    assert_eq!(stdout_json(&output)["allow_write"], true);
    let config = Config::load(&fixture.config).unwrap();
    let project = &config.projects["writable-root"];
    assert!(project.allow_write);
    assert!(!project.allow_shell && !project.allow_debug);
}

#[test]
fn project_add_non_ascii_directory_requires_explicit_name() {
    let fixture = fixture("non-ascii");
    let root = fixture.dir.join("專案");
    std::fs::create_dir_all(&root).unwrap();
    let before = std::fs::read(&fixture.config).unwrap();
    let output = project_add(&fixture.config, &["--root", root.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(
        stderr_text(&output).contains("--name"),
        "{}",
        stderr_text(&output)
    );
    assert_eq!(std::fs::read(&fixture.config).unwrap(), before);
}

#[test]
fn project_add_refuses_root_inside_state_directory() {
    let fixture = fixture("state-root");
    let state_root = fixture.dir.join("state").join("inner");
    std::fs::create_dir_all(&state_root).unwrap();
    let before = std::fs::read(&fixture.config).unwrap();
    let hash = sha256_of(&fixture.config);
    let output = project_add(
        &fixture.config,
        &[
            "--root",
            state_root.to_str().unwrap(),
            "--name",
            "inner",
            "--apply",
            "--expected-sha256",
            &hash,
        ],
    );
    assert!(!output.status.success());
    assert!(
        stderr_text(&output).contains("Project root cannot be inside the state directory"),
        "{}",
        stderr_text(&output)
    );
    assert_eq!(std::fs::read(&fixture.config).unwrap(), before);
}

#[test]
fn project_add_refuses_wrong_or_missing_hash() {
    let fixture = fixture("conflict");
    let new_root = fixture.dir.join("newproj");
    std::fs::create_dir_all(&new_root).unwrap();
    let before = std::fs::read(&fixture.config).unwrap();
    let wrong = "0".repeat(64);
    let output = project_add(
        &fixture.config,
        &[
            "--root",
            new_root.to_str().unwrap(),
            "--apply",
            "--expected-sha256",
            &wrong,
        ],
    );
    assert!(!output.status.success());
    assert!(
        stderr_text(&output).contains("CONFLICT"),
        "{}",
        stderr_text(&output)
    );
    let output = project_add(
        &fixture.config,
        &["--root", new_root.to_str().unwrap(), "--apply"],
    );
    assert!(!output.status.success());
    assert!(
        stderr_text(&output).contains("CONFLICT"),
        "{}",
        stderr_text(&output)
    );
    assert_eq!(std::fs::read(&fixture.config).unwrap(), before);
}

#[test]
fn project_add_refuses_duplicate_id_even_with_correct_hash() {
    let fixture = fixture("duplicate");
    let new_root = fixture.dir.join("newproj");
    std::fs::create_dir_all(&new_root).unwrap();
    let before = std::fs::read(&fixture.config).unwrap();
    let hash = sha256_of(&fixture.config);
    let output = project_add(
        &fixture.config,
        &[
            "--root",
            new_root.to_str().unwrap(),
            "--name",
            "app",
            "--apply",
            "--expected-sha256",
            &hash,
        ],
    );
    assert!(!output.status.success());
    assert!(
        stderr_text(&output).contains("already exists"),
        "{}",
        stderr_text(&output)
    );
    assert_eq!(std::fs::read(&fixture.config).unwrap(), before);
}

#[test]
fn project_add_refuses_overlapping_root_before_writing() {
    let fixture = fixture("overlap");
    let before = std::fs::read(&fixture.config).unwrap();
    let hash = sha256_of(&fixture.config);
    let nested = fixture.app.join("src");
    let output = project_add(
        &fixture.config,
        &[
            "--root",
            nested.to_str().unwrap(),
            "--name",
            "nested",
            "--apply",
            "--expected-sha256",
            &hash,
        ],
    );
    assert!(!output.status.success());
    assert!(
        stderr_text(&output).contains("Overlapping"),
        "{}",
        stderr_text(&output)
    );
    assert_eq!(std::fs::read(&fixture.config).unwrap(), before);
}

#[test]
fn project_add_requires_an_existing_host_config() {
    let fixture = fixture("missing-config");
    let absent = fixture.dir.join("absent.toml");
    let new_root = fixture.dir.join("newproj");
    std::fs::create_dir_all(&new_root).unwrap();
    let output = project_add(&absent, &["--root", new_root.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(!absent.exists());
}

#[test]
fn project_add_detect_languages_adds_rust_guidance_for_cargo_marker() {
    let fixture = fixture("detect");
    let new_root = fixture.dir.join("crate-root");
    std::fs::create_dir_all(&new_root).unwrap();
    std::fs::write(new_root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    let hash = sha256_of(&fixture.config);
    let output = project_add(
        &fixture.config,
        &[
            "--root",
            new_root.to_str().unwrap(),
            "--name",
            "crate-root",
            "--detect-languages",
            "--apply",
            "--expected-sha256",
            &hash,
        ],
    );
    assert!(output.status.success(), "{}", stderr_text(&output));
    let value = stdout_json(&output);
    assert_eq!(value["detected_languages"], json!(["rust"]));
    assert_eq!(value["languages_installed"], "NOT_PROBED");
    let config = Config::load(&fixture.config).unwrap();
    let languages = &config.projects["crate-root"].languages;
    assert_eq!(languages.len(), 1);
    assert_eq!(languages[0].id, "rust");
    assert_eq!(languages[0].program, "rust-analyzer");
    assert_eq!(languages[0].extensions, vec!["rs".to_string()]);
}

#[test]
fn serve_from_subdirectory_advertises_cwd_project() {
    let fixture = fixture("cwd-subdir");
    let (response, _) = initialize_stdio(
        &fixture.config,
        &["--project-from-cwd"],
        &fixture.app.join("src"),
    );
    assert_eq!(response["result"]["serverInfo"]["name"], "odin");
    assert!(
        instructions(&response).contains("Session default project: app (selected by cwd)"),
        "{}",
        instructions(&response)
    );
}

#[test]
fn serve_from_unrelated_directory_starts_without_default() {
    let fixture = fixture("cwd-unrelated");
    let unrelated = fixture.dir.join("unrelated");
    std::fs::create_dir_all(&unrelated).unwrap();
    let (response, stderr) = initialize_stdio(&fixture.config, &["--project-from-cwd"], &unrelated);
    assert_eq!(response["result"]["serverInfo"]["name"], "odin");
    assert!(!instructions(&response).contains("Session default project"));
    assert!(stderr.contains("warning"), "{stderr}");
}

#[test]
fn default_project_wins_over_cwd_selection() {
    let fixture = fixture("default-wins");
    let (response, _) = initialize_stdio(
        &fixture.config,
        &["--project-from-cwd", "--default-project", "other"],
        &fixture.app,
    );
    assert!(
        instructions(&response)
            .contains("Session default project: other (selected by default-project)"),
        "{}",
        instructions(&response)
    );
}

#[test]
fn unknown_default_project_exits_nonzero() {
    let fixture = fixture("unknown-default");
    let output = odin(&fixture.config)
        .args([
            "serve",
            "--transport",
            "stdio",
            "--default-project",
            "missing",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr_text(&output).contains("Unknown configured project"));
    assert!(output.stdout.is_empty());
}

#[test]
fn http_transport_advertises_cwd_project_with_bearer_auth() {
    let fixture = fixture("http-cwd");
    let port = free_port();
    let mut child: Child = odin(&fixture.config)
        .args([
            "serve",
            "--transport",
            "http",
            "--listen",
            &format!("127.0.0.1:{port}"),
            "--token-env",
            "ODIN_AUTO_PROJECT_TEST_TOKEN",
            "--project-from-cwd",
        ])
        .env("ODIN_AUTO_PROJECT_TEST_TOKEN", TOKEN)
        .current_dir(fixture.app.join("src"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "HTTP listener did not start");
        std::thread::sleep(Duration::from_millis(100));
    }
    let response = post_initialize(port);
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        instructions(&response).contains("Session default project: app (selected by cwd)"),
        "{}",
        instructions(&response)
    );
}

#[test]
fn setup_dry_run_reports_auto_project_and_registration_fields() {
    let fixture = fixture("setup-fields");
    let output = odin(&fixture.config)
        .args([
            "setup",
            "--client",
            "claude",
            "--project-from-cwd",
            "--default-project",
            "app",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr_text(&output));
    let value = stdout_json(&output);
    assert_eq!(value["operation"], "DRY_RUN");
    assert_eq!(value["client"], "claude");
    assert_eq!(
        value["auto_project"],
        json!({"project_from_cwd": true, "default_project": "app"})
    );
    let registration = value["registration_command"].as_array().unwrap();
    assert!(registration.iter().any(|item| item == "--project-from-cwd"));
    assert!(registration.iter().any(|item| item == "app"));
    assert!(value["default_outputs"].is_array());
    assert!(
        value["content"]
            .as_str()
            .unwrap()
            .contains("--default-project")
    );
}

#[test]
fn setup_refuses_unknown_default_project() {
    let fixture = fixture("setup-unknown");
    let output = odin(&fixture.config)
        .args(["setup", "--client", "codex", "--default-project", "nope"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr_text(&output).contains("Unknown configured project"));
}
