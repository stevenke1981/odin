//! Real native CLI processes with an isolated synthetic host profile.
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let base = std::env::temp_dir().canonicalize().unwrap();
        assert!(self.0.starts_with(&base));
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("odin-home-cli-")
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn fixture() -> Fixture {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "odin-home-cli-{}-{}-{}",
        std::process::id(),
        odin_core::unix_ms(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    Fixture(path.canonicalize().unwrap())
}
fn call(home: &Path, cwd: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_odin"))
        .args(arguments)
        .current_dir(cwd)
        .env("USERPROFILE", home)
        .env("HOME", home)
        .env("APPDATA", home.join("AppData/Roaming"))
        .env("LOCALAPPDATA", home.join("AppData/Local"))
        .output()
        .unwrap()
}
fn ok(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn default_home_is_usable_while_explicit_cwd_remains_project_specific() {
    let fixture = fixture();
    let home = fixture.0.join("home");
    let first = home.join("first");
    let second = home.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    std::fs::create_dir(home.join(".codex")).unwrap();
    std::fs::write(home.join("notes.txt"), "ordinary source").unwrap();
    std::fs::write(home.join(".codex/auth.json"), "synthetic-private-marker").unwrap();
    let created = ok(call(&home, &first, &["init"]));
    assert_eq!(
        PathBuf::from(created["root"].as_str().unwrap()),
        home.canonicalize().unwrap()
    );
    assert_eq!(created["workspace_template"], true);
    assert_eq!(created["allow_write"], false);
    let host = home.join(".config/odin/host.toml");
    let config: toml::Value = toml::from_str(&std::fs::read_to_string(&host).unwrap()).unwrap();
    assert!(config["projects"].as_table().unwrap().is_empty());
    assert_eq!(
        PathBuf::from(config["workspace"]["default_root"].as_str().unwrap()),
        home.canonicalize().unwrap()
    );
    let default = ok(call(&home, &first, &["analyze"]));
    assert_eq!(
        PathBuf::from(default["root"].as_str().unwrap()),
        home.canonicalize().unwrap()
    );
    let a = ok(call(&home, &first, &["--workspace", ".", "analyze"]));
    let b = ok(call(&home, &second, &["--workspace", ".", "analyze"]));
    assert_eq!(
        PathBuf::from(a["root"].as_str().unwrap()),
        first.canonicalize().unwrap()
    );
    assert_eq!(
        PathBuf::from(b["root"].as_str().unwrap()),
        second.canonicalize().unwrap()
    );
    assert_ne!(a["project"], b["project"]);
    for private in [".codex/auth.json", ".config/odin/host.toml"] {
        let args = serde_json::json!({"relative_path":private}).to_string();
        let denied = call(&home, &first, &["call", "read_file", "--args", &args]);
        assert!(!denied.status.success());
        assert!(
            String::from_utf8_lossy(&denied.stderr).contains("PERMISSION_DENIED"),
            "{}",
            String::from_utf8_lossy(&denied.stderr)
        );
        assert!(!String::from_utf8_lossy(&denied.stdout).contains("synthetic-private-marker"));
    }
    let client = home.join(".codex/config.toml");
    let applied = ok(call(
        &home,
        &first,
        &[
            "setup",
            "--client",
            "codex",
            "--output",
            client.to_str().unwrap(),
            "--apply",
        ],
    ));
    assert_eq!(applied["operation"], "APPLIED");
    assert!(client.is_file());
    let guidance = home.join("AGENTS.md");
    let denied = call(
        &home,
        &first,
        &["hook", "--output", guidance.to_str().unwrap(), "--apply"],
    );
    assert!(!denied.status.success());
    assert!(!guidance.exists());
    let config_before = std::fs::read(&host).unwrap();
    let duplicate = call(&home, &first, &["init"]);
    assert!(!duplicate.status.success());
    assert_eq!(std::fs::read(&host).unwrap(), config_before);
}
#[test]
fn explicit_root_keeps_legacy_registration_and_relative_config_support() {
    let fixture = fixture();
    let home = fixture.0.join("home");
    let project = home.join("project");
    let host_dir = fixture.0.join("host");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir(&host_dir).unwrap();
    let created = ok(call(
        &home,
        &host_dir,
        &[
            "--config",
            "legacy.toml",
            "init",
            "--root",
            project.to_str().unwrap(),
            "--writable",
        ],
    ));
    assert_eq!(created["workspace_template"], false);
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(host_dir.join("legacy.toml")).unwrap()).unwrap();
    assert!(config.get("workspace").is_none());
    assert_eq!(
        config["projects"]["workspace"]["allow_write"].as_bool(),
        Some(true)
    );
    let validation = ok(call(
        &home,
        &host_dir,
        &["--config", "legacy.toml", "validate"],
    ));
    assert_eq!(validation["projects"][0]["allow_write"], true);
    assert_eq!(validation["projects"][0]["allow_shell"], false);
    assert_eq!(validation["projects"][0]["allow_debug"], false);
    let denied = call(
        &home,
        &host_dir,
        &["--config", "other.toml", "--workspace", ".", "validate"],
    );
    assert!(!denied.status.success());
    assert!(!host_dir.join("other.toml").exists());
}

#[cfg(unix)]
#[test]
fn init_and_duplicate_init_preserve_existing_legacy_parent_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = fixture();
    let home = fixture.0.join("home");
    let shared_project = fixture.0.join("shared-project");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&shared_project).unwrap();
    std::fs::set_permissions(&shared_project, std::fs::Permissions::from_mode(0o755)).unwrap();
    let arguments = [
        "--config",
        "host.toml",
        "init",
        "--root",
        shared_project.to_str().unwrap(),
    ];
    ok(call(&home, &shared_project, &arguments));
    let host = shared_project.join("host.toml");
    let before = std::fs::read(&host).unwrap();
    assert_eq!(
        std::fs::metadata(&shared_project)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    let duplicate = call(&home, &shared_project, &arguments);
    assert!(!duplicate.status.success());
    assert_eq!(std::fs::read(&host).unwrap(), before);
    assert_eq!(
        std::fs::metadata(&shared_project)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}

#[cfg(unix)]
#[test]
fn standard_home_init_only_restricts_a_new_host_directory() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = fixture();
    let home = fixture.0.join("home");
    std::fs::create_dir(&home).unwrap();
    ok(call(&home, &fixture.0, &["init"]));
    let host_parent = home.join(".config/odin");
    assert_eq!(
        std::fs::metadata(&host_parent)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    std::fs::set_permissions(&host_parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let host = host_parent.join("host.toml");
    let before = std::fs::read(&host).unwrap();
    let duplicate = call(&home, &fixture.0, &["init"]);
    assert!(!duplicate.status.success());
    assert_eq!(std::fs::read(&host).unwrap(), before);
    assert_eq!(
        std::fs::metadata(&host_parent)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}

#[cfg(windows)]
fn windows_normal_path(path: &Path) -> PathBuf {
    let text = path.to_str().unwrap();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(text))
}

#[cfg(windows)]
#[test]
fn normal_absolute_home_host_path_cannot_bypass_standard_location() {
    let fixture = fixture();
    let home = windows_normal_path(&fixture.0.join("home"));
    let project = home.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let untrusted = project.join("host.toml");
    let denied = call(
        &home,
        &fixture.0,
        &["--config", untrusted.to_str().unwrap(), "init"],
    );
    assert!(!denied.status.success());
    assert!(!untrusted.exists());
    let standard = home.join(".config/odin/host.toml");
    ok(call(
        &home,
        &fixture.0,
        &["--config", standard.to_str().unwrap(), "init"],
    ));
    assert!(standard.is_file());
    ok(call(&home, &project, &["validate"]));
}

#[cfg(windows)]
#[test]
fn redirected_userprofile_is_rejected_before_canonicalization() {
    let fixture = fixture();
    let actual = windows_normal_path(&fixture.0.join("actual-home"));
    let junction = windows_normal_path(&fixture.0.join("home-junction"));
    std::fs::create_dir(&actual).unwrap();
    let created = Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&actual)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let denied = call(&junction, &fixture.0, &["init"]);
    std::fs::remove_dir(&junction).unwrap();
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("reparse point"));
    assert!(!actual.join(".config").exists());
}
