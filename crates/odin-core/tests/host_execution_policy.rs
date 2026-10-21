use odin_core::config::Config;
use std::path::{Path, PathBuf};

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("workspace")).unwrap();
    let config = directory.path().join("host.toml");
    (directory, config)
}

fn write(config: &Path, extra: &str) {
    std::fs::write(
        config,
        format!("state_dir='host-state'\n[projects.demo]\nroot='workspace'\n{extra}"),
    )
    .unwrap();
}

#[test]
fn new_execution_permissions_are_not_implicitly_enabled() {
    let (_directory, path) = fixture();
    write(&path, "");
    let config = Config::load(&path).unwrap();
    let project = &config.projects["demo"];
    assert!(!project.allow_inspections);
    assert!(!project.allow_write && !project.allow_debug && !project.allow_shell);
    assert!(project.sandbox.is_none() && project.inspections.is_empty());
}

#[test]
fn inspection_profiles_cannot_enable_source_writing_flags() {
    let (_directory, path) = fixture();
    for argument in [
        "--fix",
        "--fix=true",
        "--fix-only",
        "--add-noqa",
        "--output-file=source.rs",
    ] {
        write(
            &path,
            &format!(
                "[projects.demo.inspections.lint]\nprovider='ruff'\ncommand={{program='ruff',args=['check','{argument}']}}\n"
            ),
        );
        assert_eq!(Config::load(&path).unwrap_err().code, "INVALID_ARGUMENT");
    }
    write(
        &path,
        "[projects.demo.inspections.lint]\nprovider='eslint'\ncommand={program='eslint',args=['--fix-dry-run','--format','json']}\n",
    );
    assert!(Config::load(&path).is_ok());
}

#[test]
fn policies_can_only_reduce_inspection_permission() {
    let (_directory, path) = fixture();
    write(
        &path,
        "allow_inspections=true\n[contexts.review]\nallow_write=true\n[contexts.inspect]\nallow_inspections=true\n",
    );
    let config = Config::load(&path).unwrap();
    assert!(
        config
            .clone()
            .with_policy(Some("inspect"), None)
            .unwrap()
            .projects["demo"]
            .allow_inspections
    );
    assert!(!config.with_policy(Some("review"), None).unwrap().projects["demo"].allow_inspections);
    write(&path, "[contexts.inspect]\nallow_inspections=true\n");
    assert!(
        !Config::load(&path)
            .unwrap()
            .with_policy(Some("inspect"), None)
            .unwrap()
            .projects["demo"]
            .allow_inspections
    );
}

#[test]
fn sandbox_policy_binds_executor_and_protects_host_storage() {
    let (directory, path) = fixture();
    std::fs::write(
        directory.path().join("executor.exe"),
        b"policy parser fixture, not a real sandbox",
    )
    .unwrap();
    let hash = odin_core::sha256(b"policy parser fixture, not a real sandbox");
    let policy = format!(
        "[projects.demo.sandbox]\nexecutor='executor.exe'\nexecutor_sha256='{hash}'\nreadonly_paths=['workspace']\ntemp_root='scratch'\n"
    );
    write(&path, &policy);
    let config = Config::load(&path).unwrap();
    let sandbox = config.projects["demo"].sandbox.as_ref().unwrap();
    assert!(!sandbox.allow_ui);
    assert!(sandbox.denied_paths.contains(&path.canonicalize().unwrap()));
    assert!(sandbox.denied_paths.contains(&config.state_dir));
    std::fs::write(directory.path().join("executor.exe"), b"changed").unwrap();
    assert_eq!(Config::load(&path).unwrap_err().code, "INTEGRITY_ERROR");
}

#[test]
fn sandbox_writable_grants_cannot_cover_configuration_or_state() {
    let (directory, path) = fixture();
    std::fs::write(directory.path().join("executor.exe"), b"fixture").unwrap();
    let hash = odin_core::sha256(b"fixture");
    for grant in [".", "host-state", "host-state/child"] {
        write(
            &path,
            &format!(
                "[projects.demo.sandbox]\nexecutor='executor.exe'\nexecutor_sha256='{hash}'\ntemp_root='scratch'\nreadwrite_paths=['{grant}']\n"
            ),
        );
        assert_eq!(Config::load(&path).unwrap_err().code, "INVALID_ARGUMENT");
    }
    #[cfg(windows)]
    {
        write(
            &path,
            &format!(
                "[projects.demo.sandbox]\nexecutor='executor.exe'\nexecutor_sha256='{hash}'\ntemp_root='HOST-STATE/private'\n"
            ),
        );
        assert_eq!(Config::load(&path).unwrap_err().code, "INVALID_ARGUMENT");
    }
}
