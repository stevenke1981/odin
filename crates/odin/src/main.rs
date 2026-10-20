#![forbid(unsafe_code)]
mod http;
mod mcp;
use anyhow::{Result,Context,bail};
use clap::{Parser,Subcommand,ValueEnum};
use odin_core::config::Config;
use odin_engine::Engine;
use rmcp::ServiceExt;
use serde_json::{Value,json};
use std::{io::Write,path::{Path,PathBuf},net::SocketAddr};

#[derive(Parser)]
#[command(name="odin",version,about="Standalone Rust code intelligence and journaled editing")]
struct Cli{
    #[arg(long,global=true,default_value="odin.toml")]
    config:PathBuf,
    #[command(subcommand)]command:Commands,
}
#[derive(Subcommand)]
enum Commands{
    /// Write a new host configuration. Refuses to overwrite an existing file.
    Init{#[arg(long,default_value=".")]root:PathBuf,#[arg(long,default_value="workspace")]name:String,#[arg(long)]writable:bool},
    /// Serve MCP on stdio (default) or authenticated loopback HTTP.
    Serve{#[arg(long,value_enum,default_value="stdio")]transport:Transport,#[arg(long,default_value="127.0.0.1:9280")]listen:SocketAddr,#[arg(long,default_value="ODIN_TOKEN")]token_env:String},
    /// Print all tool contracts.
    Tools,
    /// Call the same application service used by MCP. Emits one JSON result.
    Call{tool:String,#[arg(long)]project:Option<String>,#[arg(long,default_value="{}")]args:String,#[arg(long)]args_file:Option<PathBuf>},
    /// Check configured executable presence; does not assert integration success.
    Doctor,
    /// Build the lexical index for a project.
    Index{#[arg(long)]project:Option<String>},
    /// Operator-only shared Markdown-journal recovery (explicit command).
    RecoverMemories,
}
#[derive(Clone,ValueEnum)]enum Transport{Stdio,Http}
#[tokio::main]
async fn main()->Result<()>{
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_|"warn".into()))
        .with_writer(std::io::stderr).with_ansi(false).init();
    let cli=Cli::parse();
    if let Commands::Init{root,name,writable}=&cli.command{return init(&cli.config,root,name,*writable);}
    let config=Config::load(&cli.config)?;let engine=Engine::new(config)?;
    match cli.command{
        Commands::Init{..}=>unreachable!("init is handled before configuration loading"),
        Commands::Tools=>println!("{}",serde_json::to_string_pretty(&engine.tools())?),
        Commands::Doctor=>{
            let checks=engine.projects.iter().map(|(id,p)|json!({"project":id,"root_exists":p.root.root().is_dir(),
                "languages":p.config.languages.iter().map(|l|json!({"language":l.id,"program":l.program,"found_on_path":exists_program(&l.program),"integration":"NOT_RUN"})).collect::<Vec<_>>(),
                "debuggers":p.config.debuggers.iter().map(|(name,b)|json!({"profile":name,"program":b.program,"found_on_path":exists_program(&b.program),"integration":"NOT_RUN"})).collect::<Vec<_>>(),"standalone":true})).collect::<Vec<_>>();
            println!("{}",serde_json::to_string_pretty(&json!({"checks":checks,"note":"Presence checks are not protocol or build validation"}))?);
        },
        Commands::Call{tool,project,args,args_file}=>{
            let raw=match args_file{Some(path)=>std::fs::read_to_string(path)?,None=>args};
            if raw.len()>8*1024*1024{bail!("Arguments exceed 8 MiB");}
            let value:Value=serde_json::from_str(&raw).context("Invalid JSON arguments")?;
            let project=project.or_else(||value["project"].as_str().map(str::to_string)).map(Ok).unwrap_or_else(||engine.default_project())?;
            match engine.call(project,tool,value).await{
                Ok(value)=>println!("{}",serde_json::to_string_pretty(&value)?),
                Err(error)=>{println!("{}",json!({"error":error}));return Err(error.into());},
            }
        },
        Commands::Index{project}=>{
            let project=project.map(Ok).unwrap_or_else(||engine.default_project())?;
            println!("{}",engine.call(project,"index_build".into(),json!({})).await?);
        },
        Commands::RecoverMemories=>println!("{}",engine.memories.recover()?),
        Commands::Serve{transport:Transport::Stdio,..}=>{
            // stdout belongs exclusively to the SDK JSON-RPC transport.
            let service=mcp::OdinMcp{engine}.serve(rmcp::transport::stdio()).await?;service.waiting().await?;
        },
        Commands::Serve{transport:Transport::Http,listen,token_env}=>{
            let token=std::env::var(&token_env).with_context(||format!("Set {token_env} to a random local bearer token"))?;
            http::serve(engine,listen,token).await?;
        },
    }
    Ok(())
}
fn init(path:&Path,root:&Path,name:&str,writable:bool)->Result<()>{
    odin_core::config::validate_id(name)?;
    let root=root.canonicalize()?;
    let parent=path.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new(".")).canonicalize()?;
    let mut projects=serde_json::Map::new();
    projects.insert(name.into(),json!({"root":root,"allow_write":writable,"allow_shell":false,"allow_debug":false,
        "languages":[{"id":"rust","extensions":["rs"],"program":"rust-analyzer","args":[]}],
        "commands":{"test":{"program":"cargo","args":["test","--workspace"],"timeout_secs":120},
                    "check":{"program":"cargo","args":["check","--workspace"],"timeout_secs":120}}}));
    let config:Config=serde_json::from_value(json!({"state_dir":parent.join(".odin").join("state"),"projects":projects}))?;
    let text=toml::to_string_pretty(&config)?;
    let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(path).context("Will not overwrite an existing config")?;
    file.write_all(text.as_bytes())?;file.sync_all()?;
    eprintln!("Created {}. Native writes: {writable}; shell/debug remain disabled; no external tool bridge exists.",path.display());Ok(())
}
fn exists_program(program:&str)->bool{
    let path=Path::new(program);if path.is_absolute()||path.components().count()>1{return path.is_file();}
    std::env::var_os("PATH").is_some_and(|paths|std::env::split_paths(&paths).any(|dir|{
        if dir.join(program).is_file(){return true;}
        #[cfg(windows)]{for ext in ["exe","cmd","bat"]{if dir.join(format!("{program}.{ext}")).is_file(){return true;}}}
        false
    }))
}
