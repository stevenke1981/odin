use odin_core::{config::Config, paths::SafeRoot};
use serde_json::json;
use std::path::{Path, PathBuf};

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let base = tempfile::tempdir().unwrap();
    let first = base.path().join("專案 one");
    let second = base.path().join("second");
    std::fs::create_dir_all(first.join("src")).unwrap();
    std::fs::create_dir(&second).unwrap();
    let host = base.path().join("host.toml");
    let config: Config = serde_json::from_value(json!({"state_dir":first.join("host-state"),
        "workspace":{"allowed_roots":[base.path()],"languages":[{"id":"rust","extensions":["rs"],"program":"not-a-running-backend"}]}})).unwrap();
    std::fs::write(&host, toml::to_string(&config).unwrap()).unwrap();
    (base, first, second, host)
}

#[test]
fn startup_roots_have_stable_distinct_ids_and_no_implicit_grants() {
    let (_base, a, b, host) = fixture();
    let first = Config::load_with_workspace(&host, Some(&a)).unwrap();
    let repeat = Config::load_with_workspace(&host, Some(&a.join("src/.."))).unwrap();
    let second = Config::load_with_workspace(&host, Some(&b)).unwrap();
    assert_eq!(first.projects.keys().next(), repeat.projects.keys().next());
    assert_ne!(first.projects.keys().next(), second.projects.keys().next());
    assert_eq!(first.projects.len(), 1);
    let (id, p) = first.projects.iter().next().unwrap();
    odin_core::config::validate_id(id).unwrap();
    assert_eq!(p.root, a.canonicalize().unwrap());
    assert!(
        !p.allow_write
            && !p.allow_shell
            && !p.allow_debug
            && !p.allow_inspections
            && !p.allow_formatters
    );
    assert!(p.commands.is_empty() && p.debuggers.is_empty() && p.sandbox.is_none());
    assert_eq!(p.languages[0].id, "rust");
    let safe = SafeRoot::new(&p.root, &p.deny_globs, 10000).unwrap();
    assert!(safe.resolve("host-state/memory.txt", true).is_err());
    assert!(safe.resolve(".env", true).is_err());
}

#[test]
fn outside_roots_and_unselected_template_fail_closed() {
    let (_base, _a, _b, host) = fixture();
    let outside = tempfile::tempdir().unwrap();
    assert_eq!(
        Config::load_with_workspace(&host, Some(outside.path()))
            .unwrap_err()
            .code,
        "PERMISSION_DENIED"
    );
    assert!(Config::load(&host).is_err());
    let mut config: Config = toml::from_str(&std::fs::read_to_string(&host).unwrap()).unwrap();
    config.workspace.as_mut().unwrap().allowed_roots.clear();
    std::fs::write(&host, toml::to_string(&config).unwrap()).unwrap();
    assert!(Config::load_with_workspace(&host, Some(outside.path())).is_err());
}

#[test]
fn registered_project_selected_by_nested_cwd_retains_host_policy() {
    let (_base, a, b, host) = fixture();
    let config = json!({"state_dir":b.join("state"),"projects":{"registered":{"root":a,"allow_write":true}}});
    let config: Config = serde_json::from_value(config).unwrap();
    std::fs::write(&host, toml::to_string(&config).unwrap()).unwrap();
    let loaded = Config::load_with_workspace(&host, Some(&a.join("src"))).unwrap();
    assert!(loaded.projects["registered"].allow_write);
    assert_eq!(
        loaded.projects["registered"].root,
        a.canonicalize().unwrap()
    );
    assert!(Config::load_with_workspace(&host, Some(&b)).is_err());
}

#[test]
fn repository_host_configuration_is_rejected_before_parsing() {
    let (base, _a, _b, host) = fixture();
    std::fs::write(&host, "not even valid TOML").unwrap();
    assert_eq!(
        Config::load_with_workspace(&host, Some(base.path()))
            .unwrap_err()
            .code,
        "PERMISSION_DENIED"
    );
}

#[test]
fn repo_owned_configuration_is_not_loaded_as_host_policy() {
    let (_base, a, _b, host) = fixture();
    std::fs::write(
        a.join("odin.toml"),
        "allow_write = true\nmalicious = 'not host configuration'",
    )
    .unwrap();
    let loaded = Config::load_with_workspace(&host, Some(&a)).unwrap();
    assert!(!loaded.projects.values().next().unwrap().allow_write);
    // Relative root arguments resolve from process cwd; host-relative allowed
    // roots still resolve from the host file's own directory.
    let current = std::env::current_dir().unwrap();
    let config: Config = serde_json::from_value(
        json!({"state_dir":a.join("state"),"workspace":{"allowed_roots":[current]}}),
    )
    .unwrap();
    std::fs::write(&host, toml::to_string(&config).unwrap()).unwrap();
    let loaded = Config::load_with_workspace(&host, Some(Path::new("."))).unwrap();
    assert_eq!(
        loaded.projects.values().next().unwrap().root,
        current.canonicalize().unwrap()
    );
}

#[cfg(windows)]
#[test]
fn windows_case_alias_keeps_same_state_identity() {
    let (_base, a, _b, host) = fixture();
    let first = Config::load_with_workspace(&host, Some(&a)).unwrap();
    let alias = PathBuf::from(a.to_str().unwrap().to_uppercase());
    let second = Config::load_with_workspace(&host, Some(&alias)).unwrap();
    assert_eq!(first.projects.keys().next(), second.projects.keys().next());
}

#[test]
fn workspace_selection_preserves_other_projects_and_allowed_roots_for_host_output() {
    let (base, a, b, host) = fixture();
    let third = base.path().join("third");
    std::fs::create_dir(&third).unwrap();
    let config: Config = serde_json::from_value(json!({"state_dir":base.path().join("state"),
        "projects":{"one":{"root":a},"two":{"root":b}},"workspace":{"allowed_roots":[third]}}))
    .unwrap();
    std::fs::write(&host, toml::to_string(&config).unwrap()).unwrap();
    let loaded = Config::load_with_workspace(&host, Some(&a)).unwrap();
    assert_eq!(loaded.projects.len(), 1);
    assert!(loaded.contains_workspace_path(&b.canonicalize().unwrap().join("AGENTS.md")));
    assert!(loaded.contains_workspace_path(&third.canonicalize().unwrap().join("client.toml")));
    assert!(
        !loaded
            .contains_workspace_path(&base.path().canonicalize().unwrap().join("host-output.toml"))
    );
}

#[test]
fn protected_state_and_its_descendants_cannot_be_selected_as_roots() {
    let (_base, a, _b, host) = fixture();
    let state = a.join("host-state");
    for relative in ["", "memories", "projects/other/transactions"] {
        let private = state.join(relative);
        std::fs::create_dir_all(&private).unwrap();
        assert_eq!(
            Config::load_with_workspace(&host, Some(&private))
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
    }
}

#[cfg(windows)]
#[test]
fn distinct_unicode_directory_names_never_share_workspace_state() {
    let (base, _a, _b, host) = fixture();
    let dotted = base.path().join("İ");
    let combining = base.path().join("i̇");
    std::fs::create_dir(&dotted).unwrap();
    std::fs::create_dir(&combining).unwrap();
    assert_ne!(
        dotted.canonicalize().unwrap(),
        combining.canonicalize().unwrap()
    );
    let first = Config::load_with_workspace(&host, Some(&dotted)).unwrap();
    let second = Config::load_with_workspace(&host, Some(&combining)).unwrap();
    assert_ne!(first.projects.keys().next(), second.projects.keys().next());
}
