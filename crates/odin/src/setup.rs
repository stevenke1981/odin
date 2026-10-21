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
    Agy,
    Gemini,
    Cursor,
    Vscode,
}

impl Client {
    pub fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Opencode => "opencode",
            Self::Agy => "agy",
            Self::Gemini => "gemini",
            Self::Cursor => "cursor",
            Self::Vscode => "vscode",
        }
    }
}

/// Launch options baked into a client entry. The defaults reproduce the original `serve` entry.
#[derive(Default, Clone, Copy)]
pub struct ServeOptions<'a> {
    pub context: Option<&'a str>,
    pub mode: Option<&'a str>,
    pub project_from_cwd: bool,
    pub default_project: Option<&'a str>,
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

pub fn serve_args(config: &str, options: &ServeOptions<'_>) -> Vec<String> {
    let mut args = vec!["--config".to_owned(), config.to_owned()];
    if let Some(context) = options.context {
        args.extend(["--context".to_owned(), context.to_owned()]);
    }
    if let Some(mode) = options.mode {
        args.extend(["--mode".to_owned(), mode.to_owned()]);
    }
    args.extend(["serve", "--transport", "stdio"].map(str::to_owned));
    if options.project_from_cwd {
        args.push("--project-from-cwd".to_owned());
    }
    if let Some(project) = options.default_project {
        args.extend(["--default-project".to_owned(), project.to_owned()]);
    }
    args
}

fn utf8_path<'a>(path: &'a Path, message: &str) -> Result<&'a str> {
    path.to_str().with_context(|| message.to_owned())
}

pub fn client_config(
    client: Client,
    binary: &Path,
    config: &Path,
    before: Option<&str>,
    options: &ServeOptions<'_>,
) -> Result<String> {
    let binary = utf8_path(binary, "Client config requires a UTF-8 executable path")?;
    let config = utf8_path(config, "Client config requires a UTF-8 host config path")?;
    let args = serve_args(config, options);
    let (section, entry) = match client {
        Client::Codex => return codex_config(before, binary, &args),
        Client::Claude => (
            "mcpServers",
            json!({"type":"stdio", "command": binary, "args": args}),
        ),
        Client::Opencode => {
            let command = std::iter::once(binary.to_owned())
                .chain(args)
                .collect::<Vec<_>>();
            (
                "mcp",
                json!({"type": "local", "command": command, "enabled": true}),
            )
        }
        Client::Vscode => (
            "servers",
            json!({"type":"stdio", "command": binary, "args": args}),
        ),
        Client::Agy | Client::Gemini | Client::Cursor => {
            ("mcpServers", json!({"command": binary, "args": args}))
        }
    };
    let mut document: Value = match before {
        Some(text) => {
            serde_json::from_str(text).context("Invalid JSON (JSONC is not rewritten)")?
        }
        None => json!({}),
    };
    let root = json_object(&mut document)?;
    let servers = root.entry(section).or_insert_with(|| json!({}));
    json_object(servers)?.insert("odin".into(), entry);
    Ok(serde_json::to_string_pretty(&document)? + "\n")
}

fn codex_config(before: Option<&str>, binary: &str, args: &[String]) -> Result<String> {
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
    Ok(toml::to_string_pretty(&document)?)
}

/// Native CLI registration the operator may run; Odin never executes it. `None` when the client has none.
pub fn registration_command(
    client: Client,
    binary: &Path,
    config: &Path,
    options: &ServeOptions<'_>,
) -> Result<Option<Vec<String>>> {
    let prefix: &[&str] = match client {
        Client::Claude => &["claude", "mcp", "add", "--scope", "user", "odin", "--"],
        Client::Codex => &["codex", "mcp", "add", "odin", "--"],
        Client::Agy => &["agy", "mcp", "add", "odin", "--"],
        _ => return Ok(None),
    };
    let binary = utf8_path(binary, "Client config requires a UTF-8 executable path")?;
    let config = utf8_path(config, "Client config requires a UTF-8 host config path")?;
    Ok(Some(
        prefix
            .iter()
            .map(|part| (*part).to_owned())
            .chain(std::iter::once(binary.to_owned()))
            .chain(serve_args(config, options))
            .collect(),
    ))
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn join_parts(mut base: PathBuf, parts: &[&str]) -> String {
    for part in parts {
        base.push(part);
    }
    base.to_string_lossy().into_owned()
}

/// Home-relative suggestion; a literal `~` stands in when no home directory is set.
fn home_relative(parts: &[&str]) -> String {
    home_relative_in(
        env_path(if cfg!(windows) { "USERPROFILE" } else { "HOME" }),
        parts,
    )
}

fn home_relative_in(home: Option<PathBuf>, parts: &[&str]) -> String {
    match home {
        Some(home) => join_parts(home, parts),
        None => format!("~/{}", parts.join("/")),
    }
}

fn vscode_user_mcp() -> String {
    if cfg!(windows) {
        match env_path("APPDATA") {
            Some(appdata) => join_parts(appdata, &["Code", "User", "mcp.json"]),
            None => r"%APPDATA%\Code\User\mcp.json".to_owned(),
        }
    } else if cfg!(target_os = "macos") {
        home_relative(&["Library", "Application Support", "Code", "User", "mcp.json"])
    } else {
        home_relative(&[".config", "Code", "User", "mcp.json"])
    }
}

fn suggestion(scope: &str, path: String, note: &str) -> Value {
    json!({"scope": scope, "path": path, "note": note})
}

/// Suggested locations only (never written automatically).
pub fn default_outputs(client: Client) -> Value {
    let entries = match client {
        Client::Codex => vec![suggestion(
            "user",
            match env_path("CODEX_HOME") {
                Some(home) => join_parts(home, &["config.toml"]),
                None => home_relative(&[".codex", "config.toml"]),
            },
            "TOML [mcp_servers.odin]; registration_command is the CLI alternative",
        )],
        Client::Claude => vec![suggestion(
            "project",
            ".mcp.json".into(),
            "user scope lives in ~/.claude.json, which Claude Code rewrites; use registration_command",
        )],
        Client::Opencode => vec![
            suggestion(
                "user",
                home_relative(&[".config", "opencode", "opencode.json"]),
                "JSON mcp.odin with type local",
            ),
            suggestion(
                "project",
                "opencode.json".into(),
                "JSON mcp.odin with type local",
            ),
        ],
        Client::Agy => vec![suggestion(
            "user",
            home_relative(&[".gemini", "antigravity", "mcp_config.json"]),
            "JSON mcpServers.odin; registration_command is the CLI alternative",
        )],
        Client::Gemini => vec![
            suggestion(
                "user",
                home_relative(&[".gemini", "settings.json"]),
                "JSON mcpServers.odin",
            ),
            suggestion(
                "project",
                ".gemini/settings.json".into(),
                "JSON mcpServers.odin",
            ),
        ],
        Client::Cursor => vec![
            suggestion(
                "user",
                home_relative(&[".cursor", "mcp.json"]),
                "JSON mcpServers.odin",
            ),
            suggestion("project", ".cursor/mcp.json".into(), "JSON mcpServers.odin"),
        ],
        Client::Vscode => vec![
            suggestion(
                "project",
                ".vscode/mcp.json".into(),
                "JSON servers.odin with type stdio",
            ),
            suggestion(
                "user",
                vscode_user_mcp(),
                "JSON servers.odin with type stdio",
            ),
        ],
    };
    Value::Array(entries)
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
        .unwrap_or_else(|| Path::new("."))
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

    const ALL: [Client; 7] = [
        Client::Codex,
        Client::Claude,
        Client::Opencode,
        Client::Agy,
        Client::Gemini,
        Client::Cursor,
        Client::Vscode,
    ];

    #[cfg(unix)]
    fn non_utf8_path() -> PathBuf {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(b"odin-\xff"))
    }

    #[cfg(windows)]
    fn non_utf8_path() -> PathBuf {
        use std::os::windows::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_wide(&[0xD800]))
    }

    fn registration(client: Client) -> Option<Vec<String>> {
        let options = ServeOptions {
            project_from_cwd: true,
            ..Default::default()
        };
        registration_command(client, Path::new("odin"), Path::new("host.toml"), &options).unwrap()
    }

    fn serve_tail() -> Value {
        json!(["--config", "host.toml", "serve", "--transport", "stdio"])
    }

    #[test]
    fn preserves_unrelated_json_and_toml_settings() {
        let json = client_config(
            Client::Claude,
            Path::new("odin"),
            Path::new("host.toml"),
            Some(r#"{"theme":"dark","mcpServers":{"other":{"command":"else"}}}"#),
            &ServeOptions::default(),
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
            &ServeOptions::default(),
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
                &ServeOptions::default(),
            )
            .is_err()
        );
        assert!(
            client_config(
                Client::Opencode,
                Path::new("odin"),
                Path::new("host.toml"),
                Some("// comment"),
                &ServeOptions::default(),
            )
            .is_err()
        );
        for (client, before) in [
            (Client::Agy, r#"{"mcpServers":"x"}"#),
            (Client::Gemini, r#"{"mcpServers":null}"#),
            (Client::Cursor, r#"[]"#),
            (Client::Vscode, r#"{"servers":[]}"#),
        ] {
            assert!(
                client_config(
                    client,
                    Path::new("odin"),
                    Path::new("host.toml"),
                    Some(before),
                    &ServeOptions::default(),
                )
                .is_err(),
                "{}",
                client.name()
            );
        }
    }
    #[test]
    fn client_command_preserves_restrictive_policy_selection() {
        let text = client_config(
            Client::Opencode,
            Path::new("odin"),
            Path::new("host.toml"),
            None,
            &ServeOptions {
                context: Some("reviewer"),
                mode: Some("readonly"),
                ..Default::default()
            },
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
    #[test]
    fn serve_args_orders_all_options() {
        assert_eq!(
            serve_args("host.toml", &ServeOptions::default()),
            ["--config", "host.toml", "serve", "--transport", "stdio"]
        );
        let args = serve_args(
            "host.toml",
            &ServeOptions {
                context: Some("reviewer"),
                mode: Some("readonly"),
                project_from_cwd: true,
                default_project: Some("alpha"),
            },
        );
        assert_eq!(
            args,
            [
                "--config",
                "host.toml",
                "--context",
                "reviewer",
                "--mode",
                "readonly",
                "serve",
                "--transport",
                "stdio",
                "--project-from-cwd",
                "--default-project",
                "alpha"
            ]
        );
    }
    #[test]
    fn new_json_clients_write_expected_entry_and_keep_unrelated_keys() {
        let options = ServeOptions {
            project_from_cwd: true,
            default_project: Some("alpha"),
            ..Default::default()
        };
        let mut args = serve_tail();
        args.as_array_mut().unwrap().extend([
            json!("--project-from-cwd"),
            json!("--default-project"),
            json!("alpha"),
        ]);
        for client in [Client::Agy, Client::Gemini, Client::Cursor] {
            let before = r#"{"theme":"dark","mcpServers":{"other":{"command":"else"},"odin":{"command":"stale"}}}"#;
            let text = client_config(
                client,
                Path::new("odin"),
                Path::new("host.toml"),
                Some(before),
                &options,
            )
            .unwrap();
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(value["theme"], "dark", "{}", client.name());
            assert_eq!(value["mcpServers"]["other"]["command"], "else");
            assert_eq!(
                value["mcpServers"]["odin"],
                json!({"command": "odin", "args": args}),
                "{}",
                client.name()
            );
            assert_eq!(value.as_object().unwrap().len(), 2);
            assert!(text.ends_with("}\n"));
        }
        let fresh = client_config(
            Client::Agy,
            Path::new("odin"),
            Path::new("host.toml"),
            None,
            &ServeOptions::default(),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&fresh).unwrap();
        assert_eq!(
            value,
            json!({"mcpServers": {"odin": {"command": "odin", "args": serve_tail()}}})
        );
    }
    #[test]
    fn vscode_entry_uses_servers_section_and_keeps_inputs() {
        let before = r#"{"inputs":[{"id":"token","type":"promptString"}],"servers":{"other":{"type":"http","url":"https://example.invalid"}}}"#;
        let text = client_config(
            Client::Vscode,
            Path::new("odin"),
            Path::new("host.toml"),
            Some(before),
            &ServeOptions {
                default_project: Some("alpha"),
                ..Default::default()
            },
        )
        .unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["inputs"][0]["id"], "token");
        assert_eq!(value["servers"]["other"]["type"], "http");
        assert_eq!(
            value["servers"]["odin"],
            json!({
                "type": "stdio",
                "command": "odin",
                "args": ["--config", "host.toml", "serve", "--transport", "stdio", "--default-project", "alpha"]
            })
        );
        assert!(value.get("mcpServers").is_none());
    }
    #[test]
    fn claude_and_opencode_entries_keep_their_shape() {
        let claude = client_config(
            Client::Claude,
            Path::new("odin"),
            Path::new("host.toml"),
            None,
            &ServeOptions::default(),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&claude).unwrap();
        assert_eq!(
            value,
            json!({"mcpServers": {"odin": {"type": "stdio", "command": "odin", "args": serve_tail()}}})
        );
        let opencode = client_config(
            Client::Opencode,
            Path::new("odin"),
            Path::new("host.toml"),
            None,
            &ServeOptions::default(),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&opencode).unwrap();
        assert_eq!(
            value,
            json!({"mcp": {"odin": {"type": "local", "enabled": true,
                "command": ["odin", "--config", "host.toml", "serve", "--transport", "stdio"]}}})
        );
    }
    #[test]
    fn registration_commands_match_native_cli_argv() {
        let tail = [
            "odin",
            "--config",
            "host.toml",
            "serve",
            "--transport",
            "stdio",
            "--project-from-cwd",
        ];
        let with_tail = |head: &[&str]| {
            let argv: Vec<String> = head
                .iter()
                .chain(tail.iter())
                .map(|part| part.to_string())
                .collect();
            Some(argv)
        };
        assert_eq!(
            registration(Client::Claude),
            with_tail(&["claude", "mcp", "add", "--scope", "user", "odin", "--"])
        );
        assert_eq!(
            registration(Client::Codex),
            with_tail(&["codex", "mcp", "add", "odin", "--"])
        );
        assert_eq!(
            registration(Client::Agy),
            with_tail(&["agy", "mcp", "add", "odin", "--"])
        );
        for client in [
            Client::Opencode,
            Client::Gemini,
            Client::Cursor,
            Client::Vscode,
        ] {
            assert_eq!(registration(client), None, "{}", client.name());
        }
    }
    #[test]
    fn default_outputs_cover_every_client_with_scope_and_path() {
        for client in ALL {
            let outputs = default_outputs(client);
            let entries = outputs.as_array().unwrap();
            assert!(!entries.is_empty(), "{}", client.name());
            for entry in entries {
                assert!(
                    matches!(entry["scope"].as_str(), Some("user" | "project")),
                    "{}",
                    client.name()
                );
                assert!(entry["path"].as_str().is_some_and(|p| !p.is_empty()));
                assert!(entry["note"].as_str().is_some());
            }
        }
        assert_eq!(default_outputs(Client::Claude)[0]["path"], ".mcp.json");
        assert_eq!(
            default_outputs(Client::Vscode)[0]["path"],
            ".vscode/mcp.json"
        );
    }
    #[test]
    fn home_fallback_keeps_tilde_prefix() {
        assert_eq!(
            home_relative_in(None, &[".cursor", "mcp.json"]),
            "~/.cursor/mcp.json"
        );
        let home = PathBuf::from("base");
        assert_eq!(
            home_relative_in(Some(home.clone()), &[".cursor", "mcp.json"]),
            home.join(".cursor")
                .join("mcp.json")
                .to_string_lossy()
                .into_owned()
        );
    }
    #[test]
    fn client_names_match_cli_values() {
        for client in ALL {
            assert_eq!(
                client.to_possible_value().unwrap().get_name(),
                client.name()
            );
        }
    }
    #[test]
    fn non_utf8_paths_are_rejected() {
        let bad = non_utf8_path();
        let options = ServeOptions::default();
        assert!(client_config(Client::Agy, &bad, Path::new("host.toml"), None, &options).is_err());
        assert!(client_config(Client::Claude, Path::new("odin"), &bad, None, &options).is_err());
        assert!(
            registration_command(Client::Claude, &bad, Path::new("host.toml"), &options).is_err()
        );
        assert!(registration_command(Client::Agy, Path::new("odin"), &bad, &options).is_err());
    }
}
