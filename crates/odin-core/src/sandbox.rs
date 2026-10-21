//! Explicit host-selected MXC ProcessContainer launch. Never retries without isolation.
use crate::{Fault, Result, process::configure_child, sandbox_policy::MxcSandboxConfig, sha256};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::{NamedTempFile, TempDir};

#[must_use = "Retain the policy and scratch guard until the entire sandbox tree exits"]
pub struct SandboxGuard {
    _policy: NamedTempFile,
    _scratch: TempDir,
}

#[must_use = "Use the prepared command and keep its guard through process cleanup"]
pub struct SandboxLaunch {
    pub command: Command,
    /// Keep until the child and its process tree have been reaped.
    pub guard: SandboxGuard,
}

fn invalid(message: impl Into<String>) -> Fault {
    Fault::new("SANDBOX_POLICY_INVALID", message)
}

/// Return a local, canonical path and reject links/reparse aliases in every input component.
fn canonical(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid("Sandbox paths must be absolute"));
    }
    let text = path
        .to_str()
        .ok_or_else(|| invalid("Sandbox paths must be UTF-8"))?;
    if text
        .split(['\\', '/'])
        .any(|part| part == ".." || part == ".")
    {
        return Err(invalid("Sandbox paths cannot contain dot components"));
    }
    let mut walked = PathBuf::new();
    for component in path.components() {
        walked.push(component.as_os_str());
        if let Component::Normal(name) = component {
            let name = name
                .to_str()
                .ok_or_else(|| invalid("Invalid sandbox path"))?;
            if name.contains(':') || name.ends_with(['.', ' ']) {
                return Err(invalid("Ambiguous sandbox path component"));
            }
            let metadata = fs::symlink_metadata(&walked)?;
            if metadata.file_type().is_symlink() {
                return Err(invalid("Sandbox paths cannot traverse symbolic links"));
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(invalid("Sandbox paths cannot traverse reparse points"));
                }
            }
        }
    }
    let resolved = fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let text = resolved
            .to_str()
            .ok_or_else(|| invalid("Invalid canonical sandbox path"))?;
        let text = text.strip_prefix(r"\\?\").unwrap_or(text);
        if text.starts_with(r"\\") || text.as_bytes().get(1) != Some(&b':') {
            return Err(invalid("Sandbox paths must be on a local Windows drive"));
        }
        Ok(PathBuf::from(text))
    }
    #[cfg(not(windows))]
    Ok(resolved)
}

fn within(path: &Path, parent: &Path) -> bool {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().to_lowercase())
            .starts_with(PathBuf::from(parent.to_string_lossy().to_lowercase()))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(parent)
    }
}
fn overlaps(left: &Path, right: &Path) -> bool {
    within(left, right) || within(right, left)
}

fn paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut result: Vec<PathBuf> = Vec::new();
    for path in paths {
        let path = canonical(path)?;
        if result
            .iter()
            .any(|prior| within(&path, prior) && within(prior, &path))
        {
            return Err(invalid("Duplicate or aliased sandbox grant"));
        }
        result.push(path);
    }
    Ok(result)
}

fn resolve_program(program: &str) -> Result<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() {
        return canonical(path);
    }
    if path.components().count() != 1 {
        return Err(invalid("Relative executable paths are not allowed"));
    }
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let direct = directory.join(program);
        if direct.is_file() {
            return canonical(&direct);
        }
        #[cfg(windows)]
        for extension in ["exe", "com"] {
            let candidate = directory.join(format!("{program}.{extension}"));
            if candidate.is_file() {
                return canonical(&candidate);
            }
        }
    }
    Err(Fault::new(
        "SANDBOX_EXECUTABLE_MISSING",
        "Configured workload executable was not found",
    ))
}

/// Probe is a trusted, hash-pinned native operation; its output and duration are bounded.
fn probe(executor: &Path) -> Result<Value> {
    let mut command = Command::new(executor);
    configure_child(&mut command);
    let mut child = command
        .arg("--probe")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("Missing probe output"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("Missing probe diagnostic output"))?;
    let read = |stream: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut data = Vec::new();
            stream.take(65_537).read_to_end(&mut data).map(|_| data)
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out.join();
            let _ = err.join();
            return Err(Fault::new(
                "SANDBOX_UNAVAILABLE",
                "Native sandbox probe timed out",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = out
        .join()
        .map_err(|_| invalid("Sandbox probe output reader failed"))??;
    let errors = err
        .join()
        .map_err(|_| invalid("Sandbox probe diagnostic reader failed"))??;
    if !status.success() || output.len() > 65_536 || errors.len() > 65_536 || !errors.is_empty() {
        return Err(Fault::new(
            "SANDBOX_UNAVAILABLE",
            "Native sandbox probe failed or emitted unexpected diagnostics",
        ));
    }
    let value: Value = serde_json::from_slice(&output)?;
    // Do not permit automatic DACL mutation or another containment tier. A host
    // with BFS available requires a separately reviewed implementation.
    if value["tier"] != "base-container"
        || value["needsDaclAugmentation"] != false
        || value["warnings"]
            .as_array()
            .is_none_or(|warnings| !warnings.is_empty())
        || value["probes"]["baseContainerApiPresent"] != true
        || value["probes"]["baseContainerSupportsDenyPaths"] != true
        || value["probes"]["bfscfgPresent"] != false
        || value["probes"]["bfsCompiledIn"] != false
    {
        return Err(Fault::new(
            "SANDBOX_UNAVAILABLE",
            "Required base-container isolation without fallback is unavailable",
        ));
    }
    Ok(value)
}

/// Generate a private immutable JSON policy and a native CLI command. This is
/// Windows-only; unsupported platforms and every failed precondition return Err.
/// The caller must add trusted host config/state to denied_paths, choose stdio,
/// attach ProcessTree, and retain guard for the complete child lifetime.
pub fn prepare(
    config: &MxcSandboxConfig,
    program: &str,
    args: &[String],
    cwd: &Path,
    policy_dir: &Path,
    timeout: Option<Duration>,
) -> Result<SandboxLaunch> {
    if !cfg!(windows) {
        return Err(Fault::new(
            "SANDBOX_UNAVAILABLE",
            "This sandbox implementation requires Windows ProcessContainer",
        ));
    }
    if config.executor_sha256.len() != 64
        || !config
            .executor_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err(invalid(
            "A SHA-256 pin is required for the sandbox executor",
        ));
    }
    let executor = canonical(&config.executor)?;
    if !executor.is_file()
        || !sha256(&fs::read(&executor)?).eq_ignore_ascii_case(&config.executor_sha256)
    {
        return Err(Fault::new(
            "SANDBOX_EXECUTOR_MISMATCH",
            "Sandbox executor is missing or its SHA-256 changed",
        ));
    }
    let cwd = canonical(cwd)?;
    let policy_dir = canonical(policy_dir)?;
    let temp_root = canonical(&config.temp_root)?;
    if !cwd.is_dir() || !policy_dir.is_dir() || !temp_root.is_dir() {
        return Err(invalid(
            "Cwd, policy directory and dedicated temp root must exist as directories",
        ));
    }
    let readonly = paths(&config.readonly_paths)?;
    let mut writable = paths(&config.readwrite_paths)?;
    let mut denied = paths(&config.denied_paths)?;
    if overlaps(&temp_root, &cwd)
        || overlaps(&temp_root, &policy_dir)
        || temp_root.parent().is_none()
    {
        return Err(invalid(
            "Sandbox scratch root cannot overlap workspace or policy storage",
        ));
    }
    for key in [
        "USERPROFILE",
        "HOME",
        "LOCALAPPDATA",
        "APPDATA",
        "TEMP",
        "TMP",
        "SYSTEMROOT",
    ] {
        if let Some(path) =
            std::env::var_os(key).and_then(|value| canonical(Path::new(&value)).ok())
            && within(&path, &temp_root)
        {
            return Err(invalid(
                "Sandbox scratch root must be a dedicated subdirectory, not a system/user root",
            ));
        }
    }
    for grant in &writable {
        if overlaps(&policy_dir, grant)
            || within(&executor, grant)
            || overlaps(grant, &temp_root)
            || denied.iter().any(|deny| overlaps(grant, deny))
            || readonly.iter().any(|read| overlaps(grant, read))
        {
            return Err(invalid(
                "Writable sandbox grants overlap protected paths or read-only grants",
            ));
        }
    }
    if denied.iter().any(|deny| overlaps(deny, &temp_root))
        || readonly
            .iter()
            .any(|read| within(read, &policy_dir) || denied.iter().any(|deny| within(read, deny)))
    {
        return Err(invalid("Sandbox grants conflict with denied paths"));
    }
    let target = resolve_program(program)?;
    if !target.is_file()
        || !readonly
            .iter()
            .chain(&writable)
            .any(|grant| within(&target, grant))
        || denied.iter().any(|deny| within(&target, deny))
    {
        return Err(invalid(
            "Workload executable must be within an explicit readable grant",
        ));
    }
    if !readonly
        .iter()
        .chain(&writable)
        .any(|grant| within(&cwd, grant))
        || denied.iter().any(|deny| within(&cwd, deny))
    {
        return Err(invalid("Workload cwd must be within an explicit grant"));
    }
    probe(&executor)?;
    let scratch = tempfile::Builder::new()
        .prefix("odin-mxc-")
        .tempdir_in(&temp_root)?;
    let scratch_path = canonical(scratch.path())?;
    writable.push(scratch_path.clone());
    if !denied.iter().any(|deny| within(&policy_dir, deny)) {
        denied.push(policy_dir.clone());
    }
    let mut environment = Vec::new();
    let mut runtime = Command::new(&target);
    configure_child(&mut runtime);
    for (key, value) in runtime.get_envs() {
        if let Some(value) = value {
            let key = key
                .to_str()
                .ok_or_else(|| invalid("Invalid runtime environment key"))?;
            if [
                "HOME",
                "USERPROFILE",
                "APPDATA",
                "LOCALAPPDATA",
                "TEMP",
                "TMP",
            ]
            .iter()
            .any(|name| key.eq_ignore_ascii_case(name))
            {
                continue;
            }
            let value = value
                .to_str()
                .ok_or_else(|| invalid("Invalid runtime environment value"))?;
            environment.push(format!("{key}={value}"));
        }
    }
    let scratch_text = scratch_path
        .to_str()
        .ok_or_else(|| invalid("Invalid scratch path"))?;
    for key in [
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
    ] {
        environment.push(format!("{key}={scratch_text}"));
    }
    let milliseconds = match timeout {
        Some(timeout) => {
            let milliseconds = u64::try_from(timeout.as_millis())
                .map_err(|_| invalid("Sandbox timeout too large"))?;
            if milliseconds == 0 {
                return Err(invalid(
                    "Explicit sandbox timeout must be at least one millisecond",
                ));
            }
            milliseconds
        }
        None => 0,
    };
    let policy = json!({"version":"1.0.0", "containment":"processcontainer",
        "lifecycle":{"destroyOnExit":true,"preservePolicy":false},
        "process":{"cwd":cwd,"env":environment,"inheritDefaultEnv":false,"timeout":milliseconds},
        "filesystem":{"readonlyPaths":readonly,"readwritePaths":writable,"deniedPaths":denied},
        "fallback":{"allowDaclMutation":false},
        "network":{"egress":{"default":"deny"},"ingress":{"default":"deny","hostLoopback":"deny"}},
        "ui":{"disable":!config.allow_ui,"clipboard":"none","injection":false},"telemetry":{"enabled":false}});
    let mut policy_file = tempfile::Builder::new()
        .prefix("odin-mxc-policy-")
        .suffix(".json")
        .tempfile_in(&policy_dir)?;
    policy_file.write_all(&serde_json::to_vec(&policy)?)?;
    policy_file.as_file().sync_all()?;
    let mut command = Command::new(&executor);
    configure_child(&mut command);
    command
        .arg("--config")
        .arg(policy_file.path())
        .arg("--")
        .arg(&target)
        .args(args)
        .current_dir(&policy_dir);
    Ok(SandboxLaunch {
        command,
        guard: SandboxGuard {
            _policy: policy_file,
            _scratch: scratch,
        },
    })
}
