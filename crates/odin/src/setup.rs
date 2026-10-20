//! Operator-only client configuration and prompt output. Never executes config values.
use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use odin_core::sha256;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const MAX_CONFIG_BYTES: u64 = 2 * 1024 * 1024;
const HOOK_BEGIN: &str = "<!-- ODIN MANAGED GUIDANCE BEGIN -->";
const HOOK_END: &str = "<!-- ODIN MANAGED GUIDANCE END -->";

#[derive(Clone, Copy, ValueEnum)]
pub enum Client {
    Codex,
    Claude,
    Opencode,
}

pub fn read_output(path: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_CONFIG_BYTES {
                bail!("Output must be a regular file of at most 2 MiB; symbolic links are refused");
            }
            Ok(Some(
                fs::read_to_string(path).context("Client config must be UTF-8")?,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn json_object(value: &mut Value) -> Result<&mut serde_json::Map<String, Value>> {
    value
        .as_object_mut()
        .context("Client configuration sections must be objects")
}

fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

pub fn client_config(
    client: Client,
    binary: &Path,
    config: &Path,
    before: Option<&str>,
    context: Option<&str>,
    mode: Option<&str>,
) -> Result<String> {
    let binary = binary
        .to_str()
        .context("Client config requires a UTF-8 executable path")?;
    let config = config
        .to_str()
        .context("Client config requires a UTF-8 host config path")?;
    let mut args = vec!["--config", config];
    if let Some(context) = context {
        args.extend(["--context", context]);
    }
    if let Some(mode) = mode {
        args.extend(["--mode", mode]);
    }
    args.extend(["serve", "--transport", "stdio"]);
    if matches!(client, Client::Codex) {
        let mut document: toml::Value =
            toml::from_str(before.unwrap_or("")).context("Invalid Codex TOML")?;
        let root = document
            .as_table_mut()
            .context("Codex config must be a TOML table")?;
        let servers = root
            .entry("mcp_servers")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .context("mcp_servers must be a table")?;
        let entry = json!({"command": binary, "args": args});
        servers.insert("odin".into(), toml::Value::try_from(entry)?);
        return Ok(toml::to_string_pretty(&document)?);
    }
    let mut document: Value = match before {
        Some(text) => {
            serde_json::from_str(text).context("Invalid JSON (JSONC is not rewritten)")?
        }
        None => json!({}),
    };
    let root = json_object(&mut document)?;
    let (section, entry) = if matches!(client, Client::Claude) {
        (
            "mcpServers",
            json!({"type":"stdio", "command": binary, "args": args}),
        )
    } else {
        let command = std::iter::once(binary).chain(args).collect::<Vec<_>>();
        (
            "mcp",
            json!({"type": "local", "command": command, "enabled": true}),
        )
    };
    let servers = root.entry(section).or_insert_with(|| json!({}));
    json_object(servers)?.insert("odin".into(), entry);
    Ok(serde_json::to_string_pretty(&document)? + "\n")
}

pub fn hook_content(before: &str, project: &str, disable: bool) -> Result<String> {
    let begins = before.matches(HOOK_BEGIN).count();
    let ends = before.matches(HOOK_END).count();
    if begins > 1 || ends != begins {
        bail!("Ambiguous or incomplete managed guidance block");
    }
    let cleaned = if let Some(start) = before.find(HOOK_BEGIN) {
        let end = before.find(HOOK_END).context("Missing managed block end")?;
        if end < start {
            bail!("Invalid managed guidance block order");
        }
        let end = end + HOOK_END.len();
        format!("{}{}", &before[..start], &before[end..])
    } else {
        before.to_owned()
    };
    if disable {
        return Ok(cleaned);
    }
    let guidance = format!(
        "{HOOK_BEGIN}\nUse Odin with explicit project {project}. Check get_capabilities before semantic work. Read symbols/references before editing when supported. Prepare changes with expected SHA-256, review the transaction, then apply. Report actual command exit codes. Missing backend capability must remain explicit. Child agents need their own MCP connection; no shared tools are implied.\n{HOOK_END}"
    );
    Ok(format!("{cleaned}{guidance}"))
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Compare-and-swap a host file, preserving an immutable original backup.
/// This is not a native project edit and grants no Odin permissions.
pub fn emit_or_apply(
    output: Option<&Path>,
    before: Option<&str>,
    after: &str,
    fragment: &str,
    apply: bool,
    expected: Option<&str>,
) -> Result<Value> {
    let actual = before.map(|text| sha256(text.as_bytes()));
    if !apply {
        return Ok(
            json!({"operation":"DRY_RUN", "output":output, "expected_sha256":actual,
            "new_sha256":sha256(after.as_bytes()), "changed":before != Some(after),
            "content":fragment, "existing_values_included":false,
            "note":"Managed entry preview only; unrelated settings are preserved as data. Formatting/comments may change. No client was launched or configured."}),
        );
    }
    let output = output.context("--apply requires an explicit --output host file")?;
    if actual.as_deref() != expected {
        bail!(
            "CONFLICT: --expected-sha256 must match the existing file; omit it only for a new file"
        );
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let name = output.file_name().context("Missing output filename")?;
    let output = parent.join(name);
    let lock = parent.join(format!("{}.odin-setup.lock", name.to_string_lossy()));
    let lock_file = create_private(&lock).context(
        "Another setup operation is in progress, or a stale setup lock requires operator review",
    )?;
    drop(lock_file);
    let _lock_cleanup = Cleanup(lock);
    if read_output(&output)?.as_deref() != before {
        bail!("CONFLICT: host file changed after preview");
    }
    if before == Some(after) {
        return Ok(json!({"operation":"UNCHANGED", "output":output, "sha256":actual}));
    }
    let backup = if let Some(text) = before {
        let hash = actual.as_deref().context("Missing original hash")?;
        let path = parent.join(format!("{}.odin-backup-{hash}", name.to_string_lossy()));
        match create_private(&path) {
            Ok(mut file) => {
                file.write_all(text.as_bytes())?;
                file.sync_all()?;
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::AlreadyExists
                    && read_output(&path)?.as_deref() == Some(text) => {}
            Err(error) => return Err(error).context("Cannot preserve original backup"),
        }
        Some(path)
    } else {
        None
    };
    if before.is_none() {
        let mut file = create_private(&output).context("CONFLICT: output already exists")?;
        file.write_all(after.as_bytes())?;
        file.sync_all()?;
    } else {
        let temporary = parent.join(format!(
            "{}.odin-pending-{}-{}",
            name.to_string_lossy(),
            std::process::id(),
            odin_core::unix_ms()
        ));
        let _temp_cleanup = Cleanup(temporary.clone());
        let mut file = create_private(&temporary)?;
        fs::set_permissions(&temporary, fs::metadata(&output)?.permissions())?;
        file.write_all(after.as_bytes())?;
        file.sync_all()?;
        drop(file);
        if read_output(&output)?.as_deref() != before {
            bail!("CONFLICT: host file changed before replacement");
        }
        fs::rename(&temporary, &output)
            .context("Atomic host config replacement failed; original backup retained")?;
    }
    Ok(
        json!({"operation":"APPLIED", "output":output,"sha256":sha256(after.as_bytes()),"backup":backup,
        "client_connection":"NOT_RUN", "host_permissions_changed":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_unrelated_json_and_toml_settings() {
        let json = client_config(
            Client::Claude,
            Path::new("odin"),
            Path::new("host.toml"),
            Some(r#"{"theme":"dark","mcpServers":{"other":{"command":"else"}}}"#),
            None,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcpServers"]["other"]["command"], "else");
        let toml = client_config(
            Client::Codex,
            Path::new("odin"),
            Path::new("host.toml"),
            Some("model = 'existing'\n[mcp_servers.other]\ncommand = 'else'\n"),
            None,
            None,
        )
        .unwrap();
        let value: toml::Value = toml::from_str(&toml).unwrap();
        assert_eq!(value["model"].as_str(), Some("existing"));
        assert_eq!(
            value["mcp_servers"]["other"]["command"].as_str(),
            Some("else")
        );
    }
    #[test]
    fn hooks_are_idempotent_and_disable_restores_surroundings() {
        let original = "Host-owned instructions.\n";
        let enabled = hook_content(original, "workspace", false).unwrap();
        assert_eq!(hook_content(&enabled, "workspace", false).unwrap(), enabled);
        assert_eq!(hook_content(&enabled, "workspace", true).unwrap(), original);
        assert!(hook_content(HOOK_BEGIN, "workspace", false).is_err());
    }
    #[test]
    fn malformed_sections_are_rejected() {
        assert!(
            client_config(
                Client::Claude,
                Path::new("odin"),
                Path::new("host.toml"),
                Some(r#"{"mcpServers":[]}"#),
                None,
                None,
            )
            .is_err()
        );
        assert!(
            client_config(
                Client::Opencode,
                Path::new("odin"),
                Path::new("host.toml"),
                Some("// comment"),
                None,
                None,
            )
            .is_err()
        );
    }
    #[test]
    fn client_command_preserves_restrictive_policy_selection() {
        let text = client_config(
            Client::Opencode,
            Path::new("odin"),
            Path::new("host.toml"),
            None,
            Some("reviewer"),
            Some("readonly"),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value["mcp"]["odin"]["command"],
            json!([
                "odin",
                "--config",
                "host.toml",
                "--context",
                "reviewer",
                "--mode",
                "readonly",
                "serve",
                "--transport",
                "stdio"
            ])
        );
    }
}
