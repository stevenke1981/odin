//! Host-authorized, immutable startup workspace binding. Repository files never
//! supply executable configuration; no MCP operation can register a new root.
use crate::{
    Fault, Result,
    config::{Config, LanguageConfig, ProjectConfig},
    sha256,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    /// Directories the host explicitly allows as parents of selected workspaces.
    pub allowed_roots: Vec<PathBuf>,
    /// Optional host-owned home view; never a catch-all project registration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_root: Option<PathBuf>,
    /// Absolute deny-only storage trees, applied to every registered/bound project.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protected_roots: Vec<PathBuf>,
    #[serde(default)]
    pub languages: Vec<LanguageConfig>,
    /// Additional denials; these cannot remove the normal secret-file defaults.
    #[serde(default)]
    pub deny_globs: Vec<String>,
}

impl WorkspaceConfig {
    pub(crate) fn normalize(&mut self, host_parent: &Path, home: Option<&Path>) -> Result<()> {
        if self.allowed_roots.is_empty() || self.allowed_roots.len() > 64 {
            return Err(Fault::invalid(
                "workspace.allowed_roots requires 1..64 directories",
            ));
        }
        if let Some(root) = &mut self.default_root {
            if !root.is_absolute() {
                return Err(Fault::invalid("workspace.default_root must be absolute"));
            }
            *root = root.canonicalize()?;
            if Some(root.as_path()) != home {
                return Err(Fault::denied(
                    "workspace.default_root must be the current user's canonical home",
                ));
            }
        }
        if self.protected_roots.len() > 64 {
            return Err(Fault::invalid(
                "workspace.protected_roots supports at most 64 paths",
            ));
        }
        for root in &mut self.protected_roots {
            if !root.is_absolute() {
                return Err(Fault::invalid("workspace.protected_roots must be absolute"));
            }
            *root =
                crate::config::canonical_storage_path(&crate::config::normalize_absolute(root))?;
        }
        for root in &mut self.allowed_roots {
            *root = if root.is_absolute() {
                root.clone()
            } else {
                host_parent.join(&root)
            }
            .canonicalize()?;
            if !root.is_dir() {
                return Err(Fault::invalid("Allowed workspace root is not a directory"));
            }
        }
        if self.default_root.as_ref().is_some_and(|root| {
            !self
                .allowed_roots
                .iter()
                .any(|allowed| root.starts_with(allowed))
        }) {
            return Err(Fault::denied(
                "workspace.default_root is outside allowed_roots",
            ));
        }
        Ok(())
    }
}

/// The actual host profile, never cwd or a repository-provided home token.
pub fn home_root() -> Result<PathBuf> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let root = std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|root| root.is_absolute())
        .ok_or_else(|| Fault::invalid("A valid absolute host home directory is required"))?;
    // Inspect the original identity before canonicalize can hide a home junction.
    let root = crate::config::canonical_storage_path(&root)?;
    if !root.is_dir() {
        return Err(Fault::invalid("Host home is not a directory"));
    }
    Ok(root)
}

pub fn default_host_config_path() -> Result<PathBuf> {
    crate::config::canonical_storage_path(&home_root()?.join(".config/odin/host.toml"))
}

/// Canonical existing/future host-file identity, rejecting original redirect ancestors.
pub fn host_config_path_identity(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    // Check before normalizing so `link/../host.toml` cannot hide a redirect.
    let identity = crate::config::canonical_storage_path(&absolute)?;
    Ok(crate::config::normalize_absolute(&identity))
}

pub(crate) fn client_config_path_in(home: &Path, client: &str) -> Result<PathBuf> {
    let path = match client {
        "codex" => home.join(".codex/config.toml"),
        "opencode" => home.join(".config/opencode/opencode.json"),
        "claude" => {
            #[cfg(windows)]
            {
                let roaming = if home_root().ok().as_deref() == Some(home) {
                    std::env::var_os("APPDATA")
                        .map(PathBuf::from)
                        .filter(|path| path.is_absolute())
                } else {
                    None
                }
                .unwrap_or_else(|| home.join("AppData/Roaming"));
                roaming.join("Claude/claude_desktop_config.json")
            }
            #[cfg(target_os = "macos")]
            {
                home.join("Library/Application Support/Claude/claude_desktop_config.json")
            }
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                home.join(".config/Claude/claude_desktop_config.json")
            }
        }
        _ => return Err(Fault::invalid("Unknown standard client")),
    };
    crate::config::canonical_storage_path(&path)
}

pub fn standard_client_config_path(client: &str) -> Result<PathBuf> {
    client_config_path_in(&home_root()?, client)
}

pub(crate) fn private_home_paths(home: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = [
        ".config",
        ".local",
        ".codex",
        ".claude",
        ".ssh",
        ".aws",
        ".azure",
        ".gnupg",
        ".kube",
        ".docker",
        ".npmrc",
        ".pypirc",
        ".git-credentials",
        ".netrc",
    ]
    .map(|name| home.join(name))
    .to_vec();
    if cfg!(windows) {
        paths.push(home.join("AppData"));
    }
    for client in ["codex", "claude", "opencode"] {
        paths.push(
            client_config_path_in(home, client)?
                .parent()
                .ok_or_else(|| Fault::invalid("Client path has no parent"))?
                .to_owned(),
        );
    }
    paths
        .into_iter()
        .map(|path| crate::config::canonical_storage_path(&path))
        .collect()
}

pub(crate) fn bind(config: &mut Config, root: &Path, host_parent: &Path) -> Result<String> {
    // Workspace arguments are relative to the launching process, never host.toml.
    let root = root.canonicalize()?;
    if !root.is_dir() {
        return Err(Fault::invalid("Workspace root must be a directory"));
    }
    // Existing registrations retain their exact host policy and root. Config's
    // subsequent full validation still checks ALL registrations for overlap.
    let mut selected = None;
    for (id, project) in &config.projects {
        let configured = if project.root.is_absolute() {
            project.root.clone()
        } else {
            host_parent.join(&project.root)
        }
        .canonicalize()?;
        if root.starts_with(configured) {
            if selected.is_some() {
                return Err(Fault::invalid("Overlapping project roots for workspace"));
            }
            selected = Some(id.clone());
        }
    }
    if let Some(id) = selected {
        return Ok(id);
    }
    let template = config.workspace.as_ref().ok_or_else(|| {
        Fault::denied("Workspace is not registered; configure trusted host workspace.allowed_roots")
    })?;
    if template.allowed_roots.is_empty() || template.allowed_roots.len() > 64 {
        return Err(Fault::invalid(
            "workspace.allowed_roots requires 1..64 directories",
        ));
    }
    let mut permitted = false;
    for allowed in &template.allowed_roots {
        let allowed = if allowed.is_absolute() {
            allowed.clone()
        } else {
            host_parent.join(allowed)
        }
        .canonicalize()?;
        if !allowed.is_dir() {
            return Err(Fault::invalid("Allowed workspace root is not a directory"));
        }
        permitted |= root.starts_with(allowed);
    }
    if !permitted {
        return Err(Fault::denied("Workspace is outside host allowed_roots"));
    }
    let identity = root
        .to_str()
        .ok_or_else(|| Fault::invalid("Workspace path must be UTF-8"))?;
    // canonicalize resolves filesystem spelling/aliases. Unicode lowercasing
    // can merge DISTINCT Windows names (e.g. dotted I vs i + combining dot).
    // Preserve the actual canonical UTF-8 identity without text normalization.
    let id = format!("ws_{}", &sha256(identity.as_bytes())[..48]);
    if config.projects.contains_key(&id) {
        return Err(Fault::conflict(
            "Generated workspace ID conflicts with a registered project",
        ));
    }
    let mut project: ProjectConfig = serde_json::from_value(serde_json::json!({
        "root":root,"languages":template.languages
    }))?;
    project
        .deny_globs
        .extend(template.deny_globs.iter().cloned());
    // New workspace profiles deliberately have no write/execute/debug grants,
    // command profiles, sandbox grants copied from another root, or repo config.
    config.projects.insert(id.clone(), project);
    Ok(id)
}
