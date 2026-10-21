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
    #[serde(default)]
    pub languages: Vec<LanguageConfig>,
    /// Additional denials; these cannot remove the normal secret-file defaults.
    #[serde(default)]
    pub deny_globs: Vec<String>,
}

impl WorkspaceConfig {
    pub(crate) fn normalize(&mut self, host_parent: &Path) -> Result<()> {
        if self.allowed_roots.is_empty() || self.allowed_roots.len() > 64 {
            return Err(Fault::invalid(
                "workspace.allowed_roots requires 1..64 directories",
            ));
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
        Ok(())
    }
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
