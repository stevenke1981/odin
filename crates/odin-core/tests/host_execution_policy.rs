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
    assert!(!project.allow_formatters && project.formatters.is_empty());
    assert!(!project.allow_write && !project.allow_debug && !project.allow_shell);
    assert!(project.sandbox.is_none() && project.inspections.is_empty());
}

#[test]
fn formatter_policy_is_explicit_reducible_and_cannot_grant_write() {
    let (_directory, path) = fixture();
    write(
        &path,
        "allow_formatters=true\n[contexts.format]\nallow_formatters=true\n[contexts.read]\n",
    );
    let config = Config::load(&path).unwrap();
    let enabled = config.clone().with_policy(Some("format"), None).unwrap();
    assert!(enabled.projects["demo"].allow_formatters);
    assert!(!enabled.projects["demo"].allow_write);
    assert!(!config.with_policy(Some("read"), None).unwrap().projects["demo"].allow_formatters);
    write(
        &path,
        "[contexts.format]\nallow_formatters=true\nallow_write=true\n",
    );
    let reduced = Config::load(&path)
        .unwrap()
        .with_policy(Some("format"), None)
        .unwrap();
    assert!(!reduced.projects["demo"].allow_formatters);
    assert!(!reduced.projects["demo"].allow_write);
}

#[test]
fn formatter_argv_is_fixed_isolated_stdin_only_and_placeholder_is_exact() {
    use odin_core::config::{CommandConfig, FormatterProfile, FormatterProvider};
    let (directory, path) = fixture();
    let args = [
        "format",
        "--isolated",
        "--no-cache",
        "--stdin-filename",
        "{relative_path}",
        "-",
    ];
    let profile = FormatterProfile {
        provider: FormatterProvider::Ruff,
        command: CommandConfig {
            program: "ruff".into(),
            args: args.map(String::from).into(),
            timeout_secs: 10,
        },
        extensions: vec!["py".into()],
    };
    profile.validate().unwrap();
    let filename = directory.path().join("workspace/-config.py");
    assert_eq!(
        profile.resolved_args(&filename).unwrap()[4],
        filename.to_str().unwrap()
    );
    assert!(profile.resolved_args(Path::new("relative.py")).is_err());
    for invalid in [
        vec!["format", "-"],
        vec![
            "format",
            "--isolated",
            "--no-cache",
            "--stdin-filename",
            "{relative_path}.py",
            "-",
        ],
        vec![
            "format",
            "--isolated",
            "--no-cache",
            "--stdin-filename",
            "{relative_path}",
            "--config",
            "evil.toml",
            "-",
        ],
        vec![
            "format",
            "--isolated",
            "--no-cache",
            "--stdin-filename",
            "{relative_path}",
            "source.py",
            "-",
        ],
        vec![
            "format",
            "--isolated",
            "--isolated",
            "--no-cache",
            "--stdin-filename",
            "{relative_path}",
            "-",
        ],
        vec![
            "format",
            "--isolated",
            "--no-cache",
            "--stdin-filename",
            "{relative_path}",
            "--line-length",
            "0",
            "-",
        ],
        vec![
            "format",
            "--isolated",
            "--no-cache",
            "--stdin-filename",
            "{relative_path}",
            "--target-version",
            "py3\n",
            "-",
        ],
        vec![
            "format",
            "--isolated",
            "--no-cache",
            "--stdin-filename",
            "{relative_path}",
            "--preview",
            "--no-preview",
            "-",
        ],
    ] {
        let mut bad = profile.clone();
        bad.command.args = invalid.into_iter().map(String::from).collect();
        assert!(bad.validate().is_err(), "{:?}", bad.command.args);
    }
    write(
        &path,
        "[projects.demo.formatters.ruff]\nprovider='ruff'\nextensions=['py']\ncommand={program='ruff',args=['format','--isolated','--no-cache','--stdin-filename','{relative_path}','-']}\n",
    );
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.projects["demo"].formatters.len(), 1);
    assert!(!loaded.projects["demo"].allow_formatters);
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

#[test]
fn enumeration_is_explicit_directory_only_and_distinct_from_content_grants() {
    let (directory, path) = fixture();
    std::fs::write(directory.path().join("executor.exe"), b"fixture").unwrap();
    std::fs::create_dir(directory.path().join("host-state")).unwrap();
    let hash = odin_core::sha256(b"fixture");
    let policy = format!(
        "[projects.demo.sandbox]\nexecutor='executor.exe'\nexecutor_sha256='{hash}'\ntemp_root='scratch'\n"
    );
    let volume = directory
        .path()
        .ancestors()
        .last()
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    write(
        &path,
        &format!("{policy}enumerate_paths=['{volume}','workspace']\n"),
    );
    let config = Config::load(&path).unwrap();
    let sandbox = config.projects["demo"].sandbox.as_ref().unwrap();
    assert_eq!(sandbox.enumerate_paths.len(), 2);
    assert!(sandbox.readonly_paths.is_empty() && sandbox.readwrite_paths.is_empty());
    assert!(!config.projects["demo"].allow_write && !config.projects["demo"].allow_shell);
    for invalid in ["executor.exe", "missing-directory", "host-state"] {
        write(&path, &format!("{policy}enumerate_paths=['{invalid}']\n"));
        assert_eq!(Config::load(&path).unwrap_err().code, "INVALID_ARGUMENT");
    }
    // A metadata-only volume root is allowed; read content at that root is not.
    write(&path, &format!("{policy}readonly_paths=['{volume}']\n"));
    assert_eq!(Config::load(&path).unwrap_err().code, "INVALID_ARGUMENT");
}
