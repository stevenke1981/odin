//! Bounded child-process lifetime, not an operating-system security sandbox.
//!
//! Windows jobs are assigned immediately after spawn. A hostile child can act
//! before assignment; callers must trust configured executables. Unix process
//! groups can likewise be escaped by privileged/cooperating descendants.
use crate::{Fault, Result};
use std::process::Command;

/// Deliberately inherit only runtime plumbing, never the parent's token bundle.
pub fn configure_child(command: &mut Command) {
    command.env_clear();
    for key in [
        "PATH",
        "HOME",
        "USERPROFILE",
        "SYSTEMROOT",
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
        "TEMP",
        "TMP",
        "APPDATA",
        "LOCALAPPDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "ProgramData",
        "ALLUSERSPROFILE",
        "RUSTUP_HOME",
        "CARGO_HOME",
        "JAVA_HOME",
        "DOTNET_ROOT",
        "DOTNET_CLI_HOME",
        "NUGET_PACKAGES",
        "GOROOT",
        "GOPATH",
        "GOCACHE",
        "GOMODCACHE",
        "GOTOOLCHAIN",
        "LANG",
        "LC_ALL",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.env("GOTELEMETRY", "off");
    command.env("DOTNET_CLI_TELEMETRY_OPTOUT", "1");
    command.env("DOTNET_GENERATE_ASPNET_CERTIFICATE", "false");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
}

/// Owns the process tree lifetime independently of stdout/stderr tasks.
pub struct ProcessTree {
    #[cfg(windows)]
    _job: win32job::Job,
    #[cfg(unix)]
    group: nix::unistd::Pid,
}
impl ProcessTree {
    pub fn attach(child: &tokio::process::Child) -> Result<Self> {
        #[cfg(windows)]
        {
            let handle = child
                .raw_handle()
                .ok_or_else(|| Fault::new("PROCESS_ERROR", "Child has no live process handle"))?;
            let mut limits = win32job::ExtendedLimitInfo::new();
            limits.limit_kill_on_job_close();
            let job = win32job::Job::create_with_limit_info(&limits).map_err(|e| {
                Fault::new("PROCESS_ERROR", format!("Cannot create process job: {e}"))
            })?;
            job.assign_process(handle as isize).map_err(|e| {
                Fault::new("PROCESS_ERROR", format!("Cannot attach process job: {e}"))
            })?;
            Ok(Self { _job: job })
        }
        #[cfg(unix)]
        {
            let id = child
                .id()
                .ok_or_else(|| Fault::new("PROCESS_ERROR", "Child has already exited"))?;
            let id = i32::try_from(id)
                .map_err(|_| Fault::new("PROCESS_ERROR", "Invalid child process group"))?;
            if id <= 1 {
                return Err(Fault::new("PROCESS_ERROR", "Invalid child process group"));
            }
            Ok(Self {
                group: nix::unistd::Pid::from_raw(id),
            })
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = child;
            Err(Fault::new(
                "CAPABILITY_UNAVAILABLE",
                "Process tree management is unavailable on this OS",
            ))
        }
    }
}
#[cfg(unix)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        let _ = nix::sys::signal::killpg(self.group, nix::sys::signal::Signal::SIGKILL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn environment_is_an_explicit_allowlist() {
        let mut command = Command::new("unused");
        command.env("ODIN_TEST_SECRET", "private");
        configure_child(&mut command);
        assert!(!command.get_envs().any(|(key, _)| key == "ODIN_TEST_SECRET"));
        assert!(
            !command
                .get_envs()
                .any(|(key, _)| key == "ODIN_TOKEN" || key == "GITHUB_TOKEN")
        );
        for (key, expected) in [
            ("GOTELEMETRY", "off"),
            ("DOTNET_CLI_TELEMETRY_OPTOUT", "1"),
            ("DOTNET_GENERATE_ASPNET_CERTIFICATE", "false"),
        ] {
            assert!(
                command
                    .get_envs()
                    .any(|(name, value)| name == key
                        && value == Some(std::ffi::OsStr::new(expected)))
            );
        }
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn closing_job_terminates_a_live_child() {
        let mut command = tokio::process::Command::new("ping.exe");
        command
            .args(["-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        configure_child(command.as_std_mut());
        let mut child = command.spawn().unwrap();
        let tree = ProcessTree::attach(&child).unwrap();
        assert!(child.try_wait().unwrap().is_none());
        drop(tree);
        // KILL_ON_JOB_CLOSE may use exit code zero. Completion before the
        // 60-second command duration, not its exit code, proves termination.
        tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
    }
}
