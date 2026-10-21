#![forbid(unsafe_code)]
mod http;
mod mcp;
mod metadata;
mod setup;
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use odin_core::config::Config;
use odin_engine::Engine;
use rmcp::ServiceExt;
use serde_json::{Value, json};
use std::{
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Parser)]
#[command(
    name = "odin",
    version,
    about = "Standalone Rust code intelligence and journaled editing"
)]
struct Cli {
    #[arg(long, global = true, default_value = "odin.toml")]
    config: PathBuf,
    #[arg(long, global = true)]
    context: Option<String>,
    #[arg(long, global = true)]
    mode: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create new host configuration. Shell/debug remain disabled.
    Init {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value = "workspace")]
        name: String,
        #[arg(long)]
        writable: bool,
    },
    /// Validate trusted host config without creating state or launching a backend.
    Validate,
    /// Print binary build identity. Requires no configuration.
    Metadata,
    /// Show declarative LSP profiles; never installs or starts backends.
    Languages {
        #[arg(long)]
        project: Option<String>,
    },
    /// Emit a client configuration; changing a named host file requires --apply.
    Setup {
        #[arg(long, value_enum)]
        client: setup::Client,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        expected_sha256: Option<String>,
    },
    /// Emit compact strategy guidance, or maintain a managed block in a host file.
    Hook {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        disable: bool,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        expected_sha256: Option<String>,
    },
    /// Gather read-only project evidence and instructions; never executes repository instructions.
    Onboard {
        #[arg(long)]
        project: Option<String>,
    },
    /// Serve MCP on stdio (default) or authenticated loopback HTTP.
    Serve {
        #[arg(long, value_enum, default_value = "stdio")]
        transport: Transport,
        #[arg(long, default_value = "127.0.0.1:9280")]
        listen: SocketAddr,
        #[arg(long, default_value = "ODIN_TOKEN")]
        token_env: String,
    },
    /// Print all tool contracts without opening projects or creating state.
    Tools,
    /// Call the same engine used by MCP. Emits one JSON result.
    Call {
        tool: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, default_value = "{}")]
        args: String,
        #[arg(long)]
        args_file: Option<PathBuf>,
    },
    /// Report executable presence; --probe-lsp performs actual LSP initialization.
    Doctor {
        #[arg(long)]
        probe_lsp: bool,
        #[arg(long)]
        probe_sandbox: bool,
    },
    /// Build the lexical index for a project.
    Index {
        #[arg(long)]
        project: Option<String>,
    },
    /// Operator-only shared Markdown-journal recovery (explicit command).
    RecoverMemories,
}
#[derive(Clone, ValueEnum)]
enum Transport {
    Stdio,
    Http,
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let cli = Cli::parse();
    match &cli.command {
        Commands::Metadata => return print_json(&metadata::build_metadata()),
        Commands::Languages { project: None } => {
            return print_json(&json!({"adapters":odin_core::languages::ADAPTERS,
                "status":"CONFIGURATION_GUIDANCE","installed":"NOT_PROBED",
                "acceptance":"NOT_RUN","automatic_installation":false}));
        }
        Commands::Tools => {
            return print_json(&serde_json::from_str(include_str!(
                "../../odin-engine/tools.json"
            ))?);
        }
        Commands::Init {
            root,
            name,
            writable,
        } => return init(&cli.config, root, name, *writable),
        _ => {}
    }
    let config =
        Config::load(&cli.config)?.with_policy(cli.context.as_deref(), cli.mode.as_deref())?;
    match &cli.command {
        Commands::Languages { project: Some(id) } => {
            let project = config.projects.get(id).context("Unknown project")?;
            return print_json(&json!({"project":id,"configured":project.languages,
                "adapters":odin_core::languages::ADAPTERS,"acceptance":"NOT_RUN",
                "note":"Registry entries are setup guidance. Use doctor --probe-lsp for initialization; consult source-bound acceptance reports for features."}));
        }
        Commands::Validate => {
            return print_json(&json!({"valid":true, "build":metadata::build_metadata(),
            "projects":config.projects.iter().map(|(id,p)|json!({"id":id,"allow_write":p.allow_write,"allow_shell":p.allow_shell,"allow_debug":p.allow_debug})).collect::<Vec<_>>(),
            "backends_started":false,"state_created":false}));
        }
        Commands::Setup {
            client,
            output,
            apply,
            expected_sha256,
        } => {
            if *apply {
                ensure_host_output(&config, output.as_deref())?;
            }
            let before = output
                .as_deref()
                .map(setup::read_output)
                .transpose()?
                .flatten();
            let binary = std::env::current_exe()?.canonicalize()?;
            let host_config = cli.config.canonicalize()?;
            let content = setup::client_config(
                *client,
                &binary,
                &host_config,
                before.as_deref(),
                cli.context.as_deref(),
                cli.mode.as_deref(),
            )?;
            let fragment = setup::client_config(
                *client,
                &binary,
                &host_config,
                None,
                cli.context.as_deref(),
                cli.mode.as_deref(),
            )?;
            return print_json(&setup::emit_or_apply(
                output.as_deref(),
                before.as_deref(),
                &content,
                &fragment,
                *apply,
                expected_sha256.as_deref(),
            )?);
        }
        Commands::Hook {
            project,
            disable,
            output,
            apply,
            expected_sha256,
        } => {
            if *apply {
                ensure_host_output(&config, output.as_deref())?;
            }
            let project = choose_project(&config, project.clone())?;
            let before = output
                .as_deref()
                .map(setup::read_output)
                .transpose()?
                .flatten();
            let content = setup::hook_content(before.as_deref().unwrap_or(""), &project, *disable)?;
            let fragment = setup::hook_content("", &project, *disable)?;
            return print_json(&setup::emit_or_apply(
                output.as_deref(),
                before.as_deref(),
                &content,
                &fragment,
                *apply,
                expected_sha256.as_deref(),
            )?);
        }
        _ => {}
    }
    let engine = Engine::new(config)?;
    match cli.command {
        Commands::Init { .. }
        | Commands::Validate
        | Commands::Metadata
        | Commands::Tools
        | Commands::Setup { .. }
        | Commands::Hook { .. } => unreachable!("handled before engine creation"),
        Commands::Doctor {
            probe_lsp,
            probe_sandbox,
        } => {
            let report = doctor(&engine, probe_lsp, probe_sandbox).await;
            let failed = report["ok"] == false;
            print_json(&report)?;
            if failed {
                bail!("Doctor detected unavailable configured backends; see structured checks");
            }
        }
        Commands::Onboard { project } => {
            let project = project
                .map(Ok)
                .unwrap_or_else(|| engine.default_project())?;
            print_json(&engine.call(project, "onboarding".into(), json!({})).await?)?;
        }
        Commands::Call {
            tool,
            project,
            args,
            args_file,
        } => {
            let raw = match args_file {
                Some(path) => {
                    if std::fs::metadata(&path)?.len() > 8 * 1024 * 1024 {
                        bail!("Arguments exceed 8 MiB");
                    }
                    std::fs::read_to_string(path)?
                }
                None => args,
            };
            if raw.len() > 8 * 1024 * 1024 {
                bail!("Arguments exceed 8 MiB");
            }
            let value: Value = serde_json::from_str(&raw).context("Invalid JSON arguments")?;
            let project = project
                .or_else(|| value["project"].as_str().map(str::to_string))
                .map(Ok)
                .unwrap_or_else(|| engine.default_project())?;
            match engine.call(project, tool, value).await {
                Ok(value) => print_json(&value)?,
                Err(error) => {
                    print_json(&json!({"error":error}))?;
                    return Err(error.into());
                }
            }
        }
        Commands::Index { project } => {
            let project = project
                .map(Ok)
                .unwrap_or_else(|| engine.default_project())?;
            print_json(
                &engine
                    .call(project, "index_build".into(), json!({}))
                    .await?,
            )?;
        }
        Commands::RecoverMemories => print_json(&engine.memories.recover()?)?,
        Commands::Serve {
            transport: Transport::Stdio,
            ..
        } => {
            // stdout belongs exclusively to the SDK JSON-RPC transport.
            let service = mcp::OdinMcp { engine }
                .serve(rmcp::transport::stdio())
                .await?;
            service.waiting().await?;
        }
        Commands::Serve {
            transport: Transport::Http,
            listen,
            token_env,
        } => {
            let token = std::env::var(&token_env)
                .with_context(|| format!("Set {token_env} to a random local bearer token"))?;
            http::serve(engine, listen, token).await?;
        }
        Commands::Languages { .. } => {
            unreachable!("language guidance returned before state creation")
        }
    }
    Ok(())
}

fn choose_project(config: &Config, project: Option<String>) -> Result<String> {
    let project = match project {
        Some(id) => id,
        None if config.projects.len() == 1 => {
            config.projects.keys().next().context("No project")?.clone()
        }
        None => bail!("Choose an explicit --project ID"),
    };
    if !config.projects.contains_key(&project) {
        bail!("Unknown configured project: {project}");
    }
    Ok(project)
}

fn ensure_host_output(config: &Config, output: Option<&Path>) -> Result<()> {
    let output = output.context("--apply requires an explicit --output host file")?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let output = parent.join(output.file_name().context("Missing output filename")?);
    if config
        .projects
        .values()
        .any(|project| output.starts_with(&project.root))
    {
        bail!(
            "--output must be outside configured project roots; use native Transactions for workspace edits"
        );
    }
    Ok(())
}

async fn doctor(engine: &Arc<Engine>, probe_lsp: bool, probe_sandbox: bool) -> Value {
    let mut checks = Vec::new();
    let mut ok = true;
    for (id, project) in &engine.projects {
        let mut languages = Vec::new();
        for language in &project.config.languages {
            let found = exists_program(&language.program);
            let mut entry =
                json!({"language":language.id,"found_on_path":found,"integration":"NOT_RUN"});
            if probe_lsp {
                match engine
                    .call(
                        id.clone(),
                        "get_capabilities".into(),
                        json!({"language":language.id,"probe":true}),
                    )
                    .await
                {
                    Ok(result) => {
                        let ready = result["languages"].as_array().is_some_and(|items| {
                            items
                                .iter()
                                .any(|l| l["id"] == language.id && l["ready"] == true)
                        });
                        entry["integration"] = if ready { "PASS" } else { "FAIL" }.into();
                        entry["ready"] = ready.into();
                        entry["error_code"] = if ready {
                            Value::Null
                        } else {
                            json!("LSP_NOT_READY")
                        };
                        ok &= ready;
                    }
                    Err(error) => {
                        entry["integration"] = "FAIL".into();
                        entry["error_code"] = error.code.into();
                        ok = false;
                    }
                }
            } else {
                ok &= found;
            }
            languages.push(entry);
        }
        let debuggers = project.config.debuggers.iter().map(|(name, backend)| {
            let found = exists_program(&backend.program); ok &= found;
            json!({"profile":name,"found_on_path":found,"integration":"NOT_RUN","enabled":project.config.allow_debug})
        }).collect::<Vec<_>>();
        let sandbox = if let Some(policy) = &project.config.sandbox {
            if probe_sandbox {
                match odin_core::sandbox::health(policy) {
                    Ok(report) => json!({"configured":true,"probe_status":"PASS","report":report}),
                    Err(error) => {
                        ok = false;
                        json!({"configured":true,"probe_status":"FAIL","error":error})
                    }
                }
            } else {
                json!({"configured":true,"probe_status":"NOT_RUN","enforcement":"required_on_spawn"})
            }
        } else {
            json!({"configured":false,"probe_status":"NOT_RUN","enforcement":"not_requested"})
        };
        checks.push(json!({"project":id,"root_exists":project.root.root().is_dir(),
            "root_readable":std::fs::read_dir(project.root.root()).is_ok(),"allow_write":project.config.allow_write,
            "write_access_probe":"NOT_RUN","languages":languages,"debuggers":debuggers,"sandbox":sandbox}));
    }
    json!({"ok":ok,"build":metadata::build_metadata(),"checks":checks,"mcp_transport_probe":"NOT_RUN",
        "note":"LSP PASS means initialize handshake only, not indexing or feature parity. DAP presence does not prove integration. No write permissions are enabled."})
}

fn init(path: &Path, root: &Path, name: &str, writable: bool) -> Result<()> {
    odin_core::config::validate_id(name)?;
    let root = root.canonicalize()?;
    if !root.is_dir() {
        bail!("Project root must be a directory");
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let mut projects = serde_json::Map::new();
    projects.insert(name.into(), json!({"root":root,"allow_write":writable,"allow_shell":false,"allow_debug":false,
        "languages":[{"id":"rust","extensions":["rs"],"program":"rust-analyzer","args":[]}],
        "commands":{"test":{"program":"cargo","args":["test","--workspace","--locked"],"timeout_secs":120},
                    "check":{"program":"cargo","args":["check","--workspace","--locked"],"timeout_secs":120}}}));
    let config: Config = serde_json::from_value(
        json!({"state_dir":parent.join(".odin").join("state"),"projects":projects}),
    )?;
    let text = toml::to_string_pretty(&config)?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .context("Will not overwrite an existing config")?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    print_json(
        &json!({"created":path,"allow_write":writable,"allow_shell":false,"allow_debug":false}),
    )
}

fn exists_program(program: &str) -> bool {
    let path = Path::new(program);
    if path.is_absolute() || path.components().count() > 1 {
        return path.is_file();
    }
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            if dir.join(program).is_file() {
                return true;
            }
            #[cfg(windows)]
            for extension in ["exe", "com", "cmd", "bat"] {
                if dir.join(format!("{program}.{extension}")).is_file() {
                    return true;
                }
            }
            false
        })
    })
}
