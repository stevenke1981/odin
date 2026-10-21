use crate::{Fault, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_state")]
    pub state_dir: PathBuf,
    #[serde(default)]
    pub limits: Limits,
    pub projects: BTreeMap<String, ProjectConfig>,
    #[serde(default)]
    pub memory_read_only_patterns: Vec<String>,
    #[serde(default)]
    pub allow_global_memory_write: bool,
    #[serde(default)]
    pub contexts: BTreeMap<String, PolicyConfig>,
    #[serde(default)]
    pub modes: BTreeMap<String, PolicyConfig>,
}
/// Host-owned declarative permission reductions. A true value can preserve a
/// host grant, never grant a permission denied by the host or earlier layer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PolicyConfig {
    pub allow_write: bool,
    pub allow_shell: bool,
    pub allow_debug: bool,
    pub allow_inspections: bool,
    pub excluded_tools: Vec<String>,
}
fn default_state() -> PathBuf {
    PathBuf::from("state")
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub max_file_bytes: usize,
    pub max_output_bytes: usize,
    pub max_files: usize,
    pub max_matches: usize,
    pub max_changes: usize,
    pub rpc_timeout_secs: u64,
    pub max_flow_steps: usize,
    pub max_flow_sessions: usize,
    pub max_session_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 2_097_152,
            max_output_bytes: 131_072,
            max_files: 10_000,
            max_matches: 200,
            max_changes: 256,
            rpc_timeout_secs: 60,
            max_flow_steps: 32,
            max_flow_sessions: 128,
            max_session_bytes: 1_048_576,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub root: PathBuf,
    #[serde(default)]
    pub allow_write: bool,
    #[serde(default)]
    pub allow_shell: bool,
    #[serde(default)]
    pub allow_debug: bool,
    #[serde(default)]
    pub allow_inspections: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<crate::sandbox_policy::MxcSandboxConfig>,
    #[serde(default)]
    pub excluded_tools: Vec<String>,
    #[serde(default = "default_deny")]
    pub deny_globs: Vec<String>,
    #[serde(default)]
    pub languages: Vec<LanguageConfig>,
    #[serde(default)]
    pub commands: BTreeMap<String, CommandConfig>,
    #[serde(default)]
    pub debuggers: BTreeMap<String, CommandConfig>,
    #[serde(default)]
    pub inspections: BTreeMap<String, InspectionProfile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionProvider {
    Clippy,
    Ruff,
    Eslint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionProfile {
    pub provider: InspectionProvider,
    pub command: CommandConfig,
    #[serde(default = "default_version_args")]
    pub version_args: Vec<String>,
}

fn default_version_args() -> Vec<String> {
    vec!["--version".to_owned()]
}
fn default_deny() -> Vec<String> {
    [
        "**/.env",
        "**/.env.*",
        "**/*.pem",
        "**/*.key",
        "**/id_rsa",
        "**/id_ed25519",
    ]
    .map(String::from)
    .to_vec()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandConfig {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}
fn default_timeout() -> u64 {
    60
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageConfig {
    pub id: String,
    pub extensions: Vec<String>,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub initialization_options: serde_json::Value,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub settings: serde_json::Value,
}
impl Config {
    pub fn with_policy(mut self, context: Option<&str>, mode: Option<&str>) -> Result<Self> {
        for (name, policies) in [(context, &self.contexts), (mode, &self.modes)] {
            if let Some(name) = name {
                let policy = policies.get(name).ok_or_else(|| {
                    Fault::new("UNKNOWN_POLICY", format!("Unknown context or mode: {name}"))
                })?;
                for project in self.projects.values_mut() {
                    project.allow_write &= policy.allow_write;
                    project.allow_shell &= policy.allow_shell;
                    project.allow_debug &= policy.allow_debug;
                    project.allow_inspections &= policy.allow_inspections;
                    project
                        .excluded_tools
                        .extend(policy.excluded_tools.iter().cloned());
                    project.excluded_tools.sort();
                    project.excluded_tools.dedup();
                }
                self.allow_global_memory_write &= policy.allow_write;
            }
        }
        Ok(self)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let path = path.canonicalize()?;
        let raw = std::fs::read_to_string(&path)?;
        let mut config: Self = toml::from_str(&raw).map_err(|e| Fault::invalid(e.to_string()))?;
        let parent = path
            .parent()
            .ok_or_else(|| Fault::invalid("Config has no parent"))?;
        if config.state_dir.is_relative() {
            config.state_dir = parent.join(&config.state_dir);
        }
        config.state_dir = canonical_storage_path(&normalize_absolute(&config.state_dir))?;
        if config.projects.is_empty() {
            return Err(Fault::invalid("At least one project is required"));
        }
        #[cfg(windows)]
        {
            let identities: std::collections::BTreeSet<_> = config
                .projects
                .keys()
                .map(|id| id.to_ascii_lowercase())
                .collect();
            if identities.len() != config.projects.len() {
                return Err(Fault::invalid(
                    "Project IDs collide on the Windows filesystem",
                ));
            }
        }
        if config.limits.max_file_bytes == 0
            || config.limits.max_file_bytes > 16 * 1024 * 1024
            || config.limits.max_output_bytes < 1024
            || config.limits.max_output_bytes > 4 * 1024 * 1024
            || config.limits.max_files == 0
            || config.limits.max_files > 100_000
            || config.limits.max_changes == 0
            || config.limits.max_changes > 1024
            || config.limits.max_matches == 0
            || config.limits.max_matches > 10_000
            || config.limits.rpc_timeout_secs == 0
            || config.limits.rpc_timeout_secs > 600
            || config.limits.max_flow_steps == 0
            || config.limits.max_flow_steps > 128
            || config.limits.max_flow_sessions == 0
            || config.limits.max_flow_sessions > 1024
            || config.limits.max_session_bytes == 0
            || config.limits.max_session_bytes > 16 * 1024 * 1024
        {
            return Err(Fault::invalid("Limits outside supported bounds"));
        }
        for (id, project) in &mut config.projects {
            validate_id(id)?;
            if project.root.is_relative() {
                project.root = parent.join(&project.root);
            }
            project.root = project.root.canonicalize()?;
            if !project.root.is_dir() {
                return Err(Fault::invalid(format!("Project {id} is not a directory")));
            }
            // Configuration and state are host-owned, even when stored under a
            // workspace. An agent must not rewrite its permissions on restart.
            for protected in [&path, &config.state_dir] {
                if let Ok(relative) = protected.strip_prefix(&project.root) {
                    if relative.as_os_str().is_empty() {
                        return Err(Fault::invalid(
                            "State directory cannot equal a project root",
                        ));
                    }
                    let relative = relative.to_string_lossy().replace('\\', "/");
                    project.deny_globs.push(literal_glob(&relative));
                }
            }
            let mut ids = std::collections::BTreeSet::new();
            for lang in &project.languages {
                validate_id(&lang.id)?;
                if !ids.insert(&lang.id) || lang.extensions.is_empty() {
                    return Err(Fault::invalid("Duplicate language id or empty extensions"));
                }
                validate_program(&lang.program)?;
            }
            for command in project.commands.values().chain(project.debuggers.values()) {
                validate_program(&command.program)?;
                if !(1..=600).contains(&command.timeout_secs) {
                    return Err(Fault::invalid("Command timeout must be 1..600"));
                }
            }
            for (name, profile) in &project.inspections {
                validate_id(name)?;
                validate_inspection(profile)?;
            }
            if let Some(sandbox) = &mut project.sandbox {
                validate_sandbox_policy(sandbox, parent, &path, &config.state_dir)?;
            }
        }
        // An overlapping root under another ID could bypass the same-file gate
        // or present a writable alias for a read-only project. Reject at startup.
        let roots = config.projects.iter().collect::<Vec<_>>();
        for (index, (left_id, left)) in roots.iter().enumerate() {
            for (right_id, right) in roots.iter().skip(index + 1) {
                if left.root.starts_with(&right.root) || right.root.starts_with(&left.root) {
                    return Err(Fault::invalid(format!(
                        "Overlapping project roots: {left_id} and {right_id}"
                    )));
                }
            }
        }
        for pattern in &config.memory_read_only_patterns {
            regex::Regex::new(pattern)?;
        }
        for name in config.contexts.keys().chain(config.modes.keys()) {
            validate_id(name)?;
        }
        Ok(config)
    }
}
pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(Fault::invalid(
            "IDs must be 1..64 ASCII letters/digits/underscore/hyphen",
        ));
    }
    Ok(())
}
fn validate_inspection(profile: &InspectionProfile) -> Result<()> {
    validate_program(&profile.command.program)?;
    if !(1..=600).contains(&profile.command.timeout_secs)
        || profile.command.args.len() > 128
        || profile.version_args.len() > 16
    {
        return Err(Fault::invalid(
            "Inspection profile exceeds execution limits",
        ));
    }
    for argument in profile.command.args.iter().chain(&profile.version_args) {
        if argument.contains(['\0', '\n', '\r']) || argument.len() > 16_384 {
            return Err(Fault::invalid("Invalid inspection argument"));
        }
        let option = argument.split('=').next().unwrap_or(argument);
        if [
            "--fix",
            "--fix-only",
            "--add-noqa",
            "--output-file",
            "--output",
            "-o",
        ]
        .contains(&option)
        {
            return Err(Fault::invalid(
                "Inspection tools may preview fixes, never write source directly",
            ));
        }
    }
    Ok(())
}

fn validate_sandbox_policy(
    policy: &mut crate::sandbox_policy::MxcSandboxConfig,
    parent: &Path,
    host_config: &Path,
    state: &Path,
) -> Result<()> {
    if policy.executor_sha256.len() != 64
        || !policy
            .executor_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Fault::invalid("Sandbox executor requires an exact SHA-256"));
    }
    policy.executor_sha256.make_ascii_lowercase();
    if policy.readonly_paths.len() + policy.readwrite_paths.len() + policy.denied_paths.len() > 256
    {
        return Err(Fault::invalid("Too many sandbox filesystem grants"));
    }
    for path in std::iter::once(&mut policy.executor)
        .chain(std::iter::once(&mut policy.temp_root))
        .chain(&mut policy.readonly_paths)
        .chain(&mut policy.readwrite_paths)
        .chain(&mut policy.denied_paths)
    {
        if path.is_relative() {
            *path = parent.join(&*path);
        }
        *path = canonical_storage_path(&normalize_absolute(path))?;
        if path.parent().is_none() {
            return Err(Fault::invalid("Sandbox cannot grant a filesystem root"));
        }
    }
    if !policy.executor.is_file() {
        return Err(Fault::new(
            "MISSING_BACKEND",
            "Sandbox executor is not installed",
        ));
    }
    if crate::sha256(&std::fs::read(&policy.executor)?) != policy.executor_sha256 {
        return Err(Fault::new(
            "INTEGRITY_ERROR",
            "Sandbox executor hash does not match host policy",
        ));
    }
    policy
        .denied_paths
        .extend([host_config.to_owned(), state.to_owned()]);
    policy.denied_paths.sort();
    policy.denied_paths.dedup();
    for denied in &policy.denied_paths {
        if storage_contains(&policy.temp_root, denied)
            || storage_contains(denied, &policy.temp_root)
        {
            return Err(Fault::invalid(
                "Sandbox scratch overlaps protected host storage",
            ));
        }
        if policy
            .readwrite_paths
            .iter()
            .any(|grant| storage_contains(denied, grant) || storage_contains(grant, denied))
        {
            return Err(Fault::invalid(
                "Sandbox writable grant overlaps a protected path",
            ));
        }
    }
    Ok(())
}

fn storage_contains(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().to_lowercase())
            .starts_with(PathBuf::from(root.to_string_lossy().to_lowercase()))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
}

fn validate_program(program: &str) -> Result<()> {
    if program.is_empty() || program.contains(['\n', '\r', '\0']) {
        return Err(Fault::invalid("Invalid executable"));
    }
    Ok(())
}

fn normalize_absolute(path: &Path) -> PathBuf {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                output.pop();
            }
            std::path::Component::CurDir => {}
            other => output.push(other.as_os_str()),
        }
    }
    output
}
pub(crate) fn canonical_storage_path(path: &Path) -> Result<PathBuf> {
    let mut existing = path;
    let mut suffix = Vec::new();
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(
                    existing
                        .file_name()
                        .ok_or_else(|| Fault::invalid("State has no existing ancestor"))?
                        .to_os_string(),
                );
                existing = existing
                    .parent()
                    .ok_or_else(|| Fault::invalid("State has no parent"))?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    for ancestor in existing.ancestors() {
        if crate::paths::is_redirect(&std::fs::symlink_metadata(ancestor)?) {
            return Err(Fault::denied("State path contains a link or reparse point"));
        }
    }
    let mut canonical = existing.canonicalize()?;
    for component in suffix.into_iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}
fn literal_glob(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if "*?[]{}\\".contains(ch) {
            output.push('\\');
        }
        output.push(ch);
    }
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_configuration_is_protected_inside_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("odin.toml");
        std::fs::write(
            &config_path,
            "state_dir = 'host-state'\n[projects.demo]\nroot = '.'\nallow_write = true\n",
        )
        .unwrap();
        let config = Config::load(&config_path).unwrap();
        let project = &config.projects["demo"];
        let root = crate::paths::SafeRoot::new(&project.root, &project.deny_globs, 10000).unwrap();
        assert!(root.read("odin.toml").is_err());
        assert!(root.resolve("host-state", true).is_err());
    }
    #[test]
    fn aliases_cannot_bypass_project_scope() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("odin.toml");
        std::fs::write(
            &config_path,
            "[projects.one]\nroot = '.'\n[projects.two]\nroot = '.'\n",
        )
        .unwrap();
        assert!(Config::load(&config_path).is_err());
    }
    #[test]
    fn context_then_mode_only_reduce_explicit_host_grants() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("host.toml");
        std::fs::write(&path, "allow_global_memory_write = true\n[projects.demo]\nroot='.'\nallow_write=true\nallow_shell=false\nallow_debug=true\n[contexts.editor]\nallow_write=true\nallow_shell=true\nallow_debug=false\nexcluded_tools=['debug_start']\n[modes.readonly]\nallow_write=false\nallow_shell=true\nallow_debug=true\nexcluded_tools=['transaction_apply']\n").unwrap();
        let config = Config::load(&path)
            .unwrap()
            .with_policy(Some("editor"), Some("readonly"))
            .unwrap();
        let project = &config.projects["demo"];
        assert!(!project.allow_write);
        assert!(!project.allow_shell);
        assert!(!project.allow_debug);
        assert!(!config.allow_global_memory_write);
        assert_eq!(project.excluded_tools, ["debug_start", "transaction_apply"]);
    }
    #[test]
    fn unknown_context_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("host.toml");
        std::fs::write(&path, "[projects.demo]\nroot='.'\n").unwrap();
        assert_eq!(
            Config::load(&path)
                .unwrap()
                .with_policy(Some("missing"), None)
                .unwrap_err()
                .code,
            "UNKNOWN_POLICY"
        );
    }
    #[cfg(windows)]
    #[test]
    fn windows_case_aliases_cannot_rewrite_host_policy_or_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("protected-host-configuration.toml");
        std::fs::create_dir(directory.path().join("host-state-storage")).unwrap();
        std::fs::write(directory.path().join(".ENV"), "secret").unwrap();
        std::fs::write(&path, "state_dir='host-state-storage'\n[projects.demo]\nroot='.'\nallow_write=true\nallow_shell=false\nallow_debug=false\n").unwrap();
        let config = Config::load(&path).unwrap();
        let project = &config.projects["demo"];
        let root = crate::paths::SafeRoot::new(&project.root, &project.deny_globs, 10000).unwrap();
        assert_eq!(
            root.read("PROTECTED-HOST-CONFIGURATION.TOML")
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
        assert_eq!(
            root.resolve("HOST-STATE-STORAGE/new", true)
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
        assert_eq!(root.read(".ENV").unwrap_err().code, "PERMISSION_DENIED");
    }
    #[cfg(windows)]
    #[test]
    fn windows_short_aliases_use_canonical_host_policy() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("protected-host-configuration.toml");
        let state = directory.path().join("host-state-storage");
        let source = directory.path().join("ordinary-long-source-name.txt");
        std::fs::create_dir(&state).unwrap();
        std::fs::write(&source, "before").unwrap();
        std::fs::write(
            &path,
            "state_dir='host-state-storage'\n[projects.demo]\nroot='.'\nallow_write=true\n",
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        let project = &config.projects["demo"];
        let root = crate::paths::SafeRoot::new(&project.root, &project.deny_globs, 10000).unwrap();
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "$fs = New-Object -ComObject Scripting.FileSystemObject; @{file=$fs.GetFile($env:ODIN_ALIAS_FILE).ShortName; directory=$fs.GetFolder($env:ODIN_ALIAS_DIRECTORY).ShortName; source=$fs.GetFile($env:ODIN_ALIAS_SOURCE).ShortName} | ConvertTo-Json -Compress"])
            .env("ODIN_ALIAS_FILE", &path).env("ODIN_ALIAS_DIRECTORY", &state).env("ODIN_ALIAS_SOURCE", &source).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let aliases: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let file = aliases["file"].as_str().unwrap();
        let directory_alias = aliases["directory"].as_str().unwrap();
        let source_alias = aliases["source"].as_str().unwrap();
        eprintln!("NTFS short aliases: file={file}, directory={directory_alias}");
        // An 8.3-disabled volume returns long names; do not claim short-name
        // coverage from such a run, although policy is still checked.
        assert_eq!(root.read(file).unwrap_err().code, "PERMISSION_DENIED");
        assert_eq!(
            root.resolve(&format!("{directory_alias}/new"), true)
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
        std::fs::write(
            &path,
            format!("state_dir='{directory_alias}'\n[projects.demo]\nroot='.'\nallow_write=true\n"),
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        let project = &config.projects["demo"];
        let root = crate::paths::SafeRoot::new(&project.root, &project.deny_globs, 10000).unwrap();
        assert_eq!(
            root.resolve("host-state-storage/new", true)
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
        let journal = tempfile::tempdir().unwrap();
        let transactions =
            crate::transaction::Transactions::new(root.clone(), journal.path(), 16).unwrap();
        let changes = ["ordinary-long-source-name.txt", source_alias]
            .iter()
            .enumerate()
            .map(|(index, name)| crate::transaction::Change {
                path: (*name).into(),
                expected_sha256: Some(crate::sha256(b"before")),
                content: Some(index.to_string()),
            })
            .collect();
        assert_eq!(
            transactions.prepare(changes, None).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
        root.write_atomic(source_alias, b"after").unwrap();
        assert_eq!(std::fs::read_to_string(source).unwrap(), "after");
    }
    #[cfg(windows)]
    #[test]
    fn windows_project_ids_cannot_share_a_state_namespace_by_case() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("a")).unwrap();
        std::fs::create_dir(directory.path().join("b")).unwrap();
        let path = directory.path().join("host.toml");
        std::fs::write(
            &path,
            "[projects.demo]\nroot='a'\n[projects.DEMO]\nroot='b'\n",
        )
        .unwrap();
        assert_eq!(Config::load(&path).unwrap_err().code, "INVALID_ARGUMENT");
    }
}
