//! Explicit host policy for the optional native MXC process boundary.
//! Presence requests enforcement; an unavailable executor must never fall back.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MxcSandboxConfig {
    pub executor: PathBuf,
    pub executor_sha256: String,
    #[serde(default)]
    pub readonly_paths: Vec<PathBuf>,
    #[serde(default)]
    pub readwrite_paths: Vec<PathBuf>,
    #[serde(default)]
    pub denied_paths: Vec<PathBuf>,
    /// Explicit query/enumeration-only paths. These never cover content reads,
    /// executable discovery or cwd readability. Native support is mandatory.
    #[serde(default)]
    pub enumerate_paths: Vec<PathBuf>,
    pub temp_root: PathBuf,
    #[serde(default)]
    pub allow_ui: bool,
}

pub type SandboxConfig = MxcSandboxConfig;
