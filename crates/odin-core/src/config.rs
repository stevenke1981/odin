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
    pub excluded_tools: Vec<String>,
    #[serde(default = "default_deny")]
    pub deny_globs: Vec<String>,
    #[serde(default)]
    pub languages: Vec<LanguageConfig>,
    #[serde(default)]
    pub commands: BTreeMap<String, CommandConfig>,
    #[serde(default)]
    pub debuggers: BTreeMap<String, CommandConfig>,
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
        config.state_dir = normalize_absolute(&config.state_dir);
        if config.projects.is_empty() {
            return Err(Fault::invalid("At least one project is required"));
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
}
