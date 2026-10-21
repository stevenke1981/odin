#![forbid(unsafe_code)]
//! Shared application service for CLI, MCP, and the read-only dashboard.
pub mod flow;
mod ide;
mod inspections;
mod occurrences;
mod refactor;
mod results;
pub mod schema;
mod shell;
mod symbols;
mod text_replace;

use odin_core::{
    Fault, Result,
    config::{Config, LanguageConfig, ProjectConfig},
    memory::Memories,
    paths::{SafeRoot, private_dir},
    search, sha256,
    store::Store,
    transaction::{Change, Transactions},
};
use odin_protocol::{
    dap::DebugAdapter,
    lsp::LspClient,
    text::{Position, Range, TextEdit, edit, offset},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub struct Project {
    pub id: String,
    pub config: ProjectConfig,
    pub root: SafeRoot,
    pub transactions: Transactions,
    pub store: Store,
    state: PathBuf,
    gate: Mutex<()>,
    results_gate: std::sync::Mutex<()>,
}
pub(crate) type FlowNamespace = Arc<Mutex<BTreeMap<String, Value>>>;
struct DebugSession {
    id: String,
    profile: String,
    adapter: Arc<DebugAdapter>,
}
pub struct Engine {
    pub config: Config,
    pub projects: BTreeMap<String, Arc<Project>>,
    pub memories: Memories,
    catalog: Vec<Value>,
    lsp: Mutex<BTreeMap<(String, String), Arc<LspClient>>>,
    debuggers: Mutex<BTreeMap<String, DebugSession>>,
    pub(crate) flows: Mutex<BTreeMap<(String, String), FlowNamespace>>,
}
impl Engine {
    pub fn new(config: Config) -> Result<Arc<Self>> {
        private_dir(&config.state_dir)?;
        let mut projects = BTreeMap::new();
        for (id, pc) in &config.projects {
            let state = config.state_dir.join("projects").join(id);
            private_dir(&state)?;
            private_dir(&state.join("results"))?;
            let root = SafeRoot::new(&pc.root, &pc.deny_globs, config.limits.max_file_bytes)?;
            let transactions = Transactions::new(
                root.clone(),
                state.join("transactions"),
                config.limits.max_changes,
            )?;
            let store = Store::new(state.join("index.sqlite3"))?;
            projects.insert(
                id.clone(),
                Arc::new(Project {
                    id: id.clone(),
                    config: pc.clone(),
                    root,
                    transactions,
                    store,
                    state,
                    gate: Mutex::new(()),
                    results_gate: std::sync::Mutex::new(()),
                }),
            );
        }
        let ids = config.projects.keys().cloned().collect::<BTreeSet<_>>();
        let writable = config
            .projects
            .iter()
            .filter(|(_, p)| p.allow_write)
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
        let memories = Memories::new(
            &config.state_dir,
            ids,
            writable,
            config.allow_global_memory_write,
            &config.memory_read_only_patterns,
            config.limits.max_file_bytes,
            config.limits.max_changes,
        )?;
        let catalog: Vec<Value> = serde_json::from_str(include_str!("../tools.json"))?;
        Ok(Arc::new(Self {
            config,
            projects,
            memories,
            catalog,
            lsp: Mutex::new(BTreeMap::new()),
            debuggers: Mutex::new(BTreeMap::new()),
            flows: Mutex::new(BTreeMap::new()),
        }))
    }
    pub fn tools(&self) -> Vec<Value> {
        self.catalog.clone()
    }
    pub fn tool(&self, name: &str) -> Option<&Value> {
        self.catalog.iter().find(|t| t["name"] == name)
    }
    pub fn project(&self, id: &str) -> Result<Arc<Project>> {
        self.projects
            .get(id)
            .cloned()
            .ok_or_else(|| Fault::invalid(format!("Unknown configured project: {id}")))
    }
    pub fn default_project(&self) -> Result<String> {
        if self.projects.len() == 1 {
            Ok(self
                .projects
                .keys()
                .next()
                .cloned()
                .ok_or_else(|| Fault::invalid("No project"))?)
        } else {
            Err(Fault::invalid(
                "Choose an explicit project ID; Odin has no shared active project",
            ))
        }
    }
    pub fn read_tool(&self, name: &str) -> bool {
        self.tool(name).is_some_and(|t| t["policy"] == "read")
            && !name.starts_with("flow_")
            && name != "query_project"
    }
    pub async fn call(
        self: &Arc<Self>,
        project: String,
        tool: String,
        args: Value,
    ) -> Result<Value> {
        let value = self.invoke(project.clone(), tool.clone(), args).await?;
        if tool == "result_read" {
            return Ok(value);
        }
        results::limit(self, &project, value)
    }
    /// Boxed boundary permits bounded flow/query delegation without recursive
    /// async future types. Public requests still enter the same policy path.
    pub fn invoke(
        self: &Arc<Self>,
        project: String,
        tool: String,
        args: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>> {
        let engine = Arc::clone(self);
        Box::pin(async move { engine.run(&project, &tool, args).await })
    }
    async fn run(self: &Arc<Self>, project_id: &str, tool: &str, mut args: Value) -> Result<Value> {
        let project = self.project(project_id)?;
        let spec = self
            .tool(tool)
            .ok_or_else(|| Fault::new("UNKNOWN_TOOL", format!("Unknown tool: {tool}")))?;
        let object = args
            .as_object_mut()
            .ok_or_else(|| Fault::invalid("Arguments must be an object"))?;
        if object.get("project").is_some_and(|v| v != project_id) {
            return Err(Fault::denied(
                "Project override disagrees with request scope",
            ));
        }
        object.insert("project".into(), project_id.into());
        if serde_json::to_vec(&args)?.len() > 8 * 1024 * 1024 {
            return Err(Fault::new("LIMIT_EXCEEDED", "Arguments exceed 8 MiB"));
        }
        schema::validate(&spec["inputSchema"], &args)?;
        let started = Instant::now();
        let fingerprint = sha256(&serde_json::to_vec(&args)?);
        let trace_id = uuid::Uuid::new_v4().to_string();
        let result = async {
            if project.config.excluded_tools.iter().any(|n| n == tool) {
                return Err(Fault::denied("Tool disabled by host policy"));
            }
            match spec["policy"].as_str().unwrap_or("") {
                "write" if !project.config.allow_write => {
                    return Err(Fault::denied("Project writes are disabled"));
                }
                "execute" if tool.starts_with("debug_") && !project.config.allow_debug => {
                    return Err(Fault::denied("Debug execution disabled"));
                }
                "execute" if tool == "execute_shell_command" && !project.config.allow_shell => {
                    return Err(Fault::denied("Command execution disabled"));
                }
                _ if matches!(tool, "run_inspections" | "prepare_inspection_fix")
                    && !project.config.allow_inspections =>
                {
                    return Err(Fault::denied("Native inspections are disabled by the host"));
                }
                _ => {}
            }
            match tool {
                "flow_run" => tokio::time::timeout(
                    Duration::from_secs(self.config.limits.rpc_timeout_secs),
                    flow::run(self, project_id, &args),
                )
                .await
                .map_err(|_| {
                    Fault::new(
                        "TIMEOUT",
                        "Flow deadline exceeded; namespace draft discarded",
                    )
                })?,
                "flow_vars" => flow::vars(self, project_id, string(&args, "session")?).await,
                "flow_clear" => flow::clear(self, project_id, string(&args, "session")?).await,
                "query_project" => {
                    let target = string(&args, "target_project")?;
                    let target_tool = string(&args, "tool")?;
                    if !self.read_tool(target_tool) {
                        return Err(Fault::denied(
                            "External project queries accept only nonrecursive native read tools",
                        ));
                    }
                    self.invoke(target.into(), target_tool.into(), args["arguments"].clone())
                        .await
                }
                _ => {
                    let _guard = project.gate.lock().await;
                    self.dispatch(&project, tool, &args).await
                }
            }
        }
        .await;
        let code = result.as_ref().err().map(|e| e.code.as_str());
        let elapsed = started.elapsed().as_millis() as u64;
        let audit = project.store.audit_with_trace(
            project_id,
            tool,
            result.is_ok(),
            elapsed,
            &fingerprint,
            (&trace_id, code),
        );
        result.and_then(|mut value|{
            if !value.is_object(){value=json!({"result":value});}
            if let Err(error)=&audit{value["audit_warning"]=json!({"code":error.code,"recorded":false});}
            let bytes=serde_json::to_vec(&value)?;
            let trace=json!({"id":trace_id,"result_sha256":sha256(&bytes),"result_bytes":bytes.len(),"elapsed_ms":elapsed});
            value["trace"]=trace;
            Ok(value)
        })
    }
    pub fn project_summaries(&self) -> Value {
        json!({"projects":self.projects.iter().map(|(id,p)|json!({"id":id,"root":p.root.root(),"allow_write":p.config.allow_write,
            "allow_shell":p.config.allow_shell,"allow_debug":p.config.allow_debug,"allow_inspections":p.config.allow_inspections,
            "languages":p.config.languages.iter().map(|l|&l.id).collect::<Vec<_>>()})).collect::<Vec<_>>()})
    }
    fn language_for(&self, project: &Project, path: &str) -> Result<LanguageConfig> {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        project
            .config
            .languages
            .iter()
            .find(|l| l.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
            .cloned()
            .ok_or_else(|| {
                Fault::new(
                    "CAPABILITY_UNAVAILABLE",
                    format!("No configured language server for .{ext}"),
                )
            })
    }
    async fn lsp_for(&self, project: &Project, path: &str) -> Result<Arc<LspClient>> {
        let language = self.language_for(project, path)?;
        self.lsp_language(project, language).await
    }
    async fn lsp_language(
        &self,
        project: &Project,
        language: LanguageConfig,
    ) -> Result<Arc<LspClient>> {
        let key = (project.id.clone(), language.id.clone());
        if let Some(client) = self.lsp.lock().await.get(&key).cloned()
            && !client.is_closed()
        {
            return Ok(client);
        }
        let client = LspClient::start_with_sandbox(
            project.root.clone(),
            language,
            Duration::from_secs(self.config.limits.rpc_timeout_secs),
            project
                .config
                .sandbox
                .as_ref()
                .map(|policy| (policy, project.state.as_path())),
        )
        .await
        .map_err(backend_error)?;
        self.lsp.lock().await.insert(key, client.clone());
        Ok(client)
    }
    async fn refresh_backends(&self, project: &Project, result: &mut Value) -> Result<()> {
        let id = result["transaction_id"]
            .as_str()
            .ok_or_else(|| Fault::new("CORRUPT_STATE", "Missing applied transaction id"))?;
        let preview = project.transactions.preview(id)?;
        let changed = preview["files"]
            .as_array()
            .map(|f| {
                f.iter()
                    .filter_map(|f| f["path"].as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let clients = self
            .lsp
            .lock()
            .await
            .iter()
            .filter(|((id, _), _)| id == &project.id)
            .map(|(_, c)| c.clone())
            .collect::<Vec<_>>();
        let mut warnings = Vec::new();
        for client in clients {
            if let Err(e) = client.refresh(&changed).await {
                warnings.push(e.to_string());
            }
        }
        result["backend_sync_warnings"] = json!(warnings);
        Ok(())
    }
    async fn stop_language_servers(&self, project: &str) -> Vec<String> {
        let clients = {
            let mut servers = self.lsp.lock().await;
            let keys = servers
                .keys()
                .filter(|(id, _)| id == project)
                .cloned()
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| servers.remove(&key))
                .collect::<Vec<_>>()
        };
        let mut warnings = Vec::new();
        for client in clients {
            if let Err(error) = client.shutdown().await {
                warnings.push(error.to_string());
            }
        }
        warnings
    }
    async fn dispatch(self: &Arc<Self>, p: &Arc<Project>, tool: &str, a: &Value) -> Result<Value> {
        let limit = &self.config.limits;
        match tool {
            "status" => Ok(
                json!({"name":"Odin","version":env!("CARGO_PKG_VERSION"),"project":p.id,
                "native_language":"Rust","standalone":true,"external_tool_bridge":false,"tool_count":self.catalog.len(),"native_or_protocol_tools":self.catalog.len(),
                "capabilities_are_backend_dependent":true,"native_read_only":!p.config.allow_write,
                "transaction_visibility":"per-file atomic, journaled multi-file recovery","sandbox":false}),
            ),
            "get_version" => Ok(
                json!({"name":"Odin","version":env!("CARGO_PKG_VERSION"),"schema_version":1,"standalone":true}),
            ),
            "get_schemas" => {
                if let Some(name) = optional_string(a, "tool") {
                    let tool = self.tool(name).ok_or_else(|| {
                        Fault::new("UNKNOWN_TOOL", format!("Unknown tool: {name}"))
                    })?;
                    Ok(json!({"schema_version":1,"tool":tool}))
                } else {
                    Ok(json!({"schema_version":1,"tools":self.catalog}))
                }
            }
            "get_capabilities" => {
                let requested = optional_string(a, "language");
                if requested
                    .is_some_and(|id| !p.config.languages.iter().any(|language| language.id == id))
                {
                    return Err(Fault::new(
                        "MISSING_BACKEND",
                        "Requested language is not configured by the host",
                    ));
                }
                let mut languages = Vec::new();
                for language in p
                    .config
                    .languages
                    .iter()
                    .filter(|language| requested.is_none_or(|id| language.id == id))
                {
                    let existing = self
                        .lsp
                        .lock()
                        .await
                        .get(&(p.id.clone(), language.id.clone()))
                        .cloned();
                    let client = if boolean(a, "probe", false) {
                        Some(self.lsp_language(p, language.clone()).await)
                    } else {
                        existing.map(Ok)
                    };
                    let mut entry = json!({"id":language.id,"extensions":language.extensions,"configured":true,"probed":client.is_some(),"ready":false});
                    match client {
                        Some(Ok(client)) => {
                            entry["ready"] = json!(!client.is_closed());
                            entry["capabilities"] = client.capabilities.clone();
                            entry["status"] = client.status().map_err(backend_error)?;
                        }
                        Some(Err(error)) => entry["error"] = serde_json::to_value(error)?,
                        None => {}
                    }
                    languages.push(entry);
                }
                let tools = self
                    .catalog
                    .iter()
                    .map(|tool| {
                        let name = tool["name"].as_str().unwrap_or("");
                        let enabled = !p.config.excluded_tools.iter().any(|item| item == name)
                            && (!matches!(name, "run_inspections" | "prepare_inspection_fix")
                                || (p.config.allow_inspections && p.config.sandbox.is_some()))
                            && match tool["policy"].as_str() {
                                Some("write") => p.config.allow_write,
                                Some("execute") if name.starts_with("debug_") => {
                                    p.config.allow_debug
                                }
                                Some("execute") if name == "run_inspections" => {
                                    p.config.allow_inspections
                                }
                                Some("execute") => p.config.allow_shell,
                                _ => true,
                            };
                        json!({"name":name,"policy":tool["policy"],"enabled":enabled})
                    })
                    .collect::<Vec<_>>();
                Ok(
                    json!({"project":p.id,"standalone":true,"external_tool_bridge":false,"semantic_available":languages.iter().any(|l|l["ready"]==true),"languages":languages,
                    "tools":tools,"write_allowed":p.config.allow_write,"shell_allowed":p.config.allow_shell,"debug_allowed":p.config.allow_debug,
                    "inspections_allowed":p.config.allow_inspections,"inspection_profiles":p.config.inspections.iter().map(|(name,profile)|json!({"name":name,"provider":profile.provider,"requires_sandbox":true,"enabled":p.config.allow_inspections&&p.config.sandbox.is_some()})).collect::<Vec<_>>(),
                    "result_retention":{"ttl_ms":results::TTL_MS,"max_results":results::MAX_RESULTS,"max_bytes":results::MAX_STORAGE_BYTES}}),
                )
            }
            "list_projects" => Ok(self.project_summaries()),
            "get_current_config" => Ok(
                json!({"project":p.id,"root":p.root.root(),"allow_write":p.config.allow_write,
                "allow_shell":p.config.allow_shell,"allow_debug":p.config.allow_debug,"allow_inspections":p.config.allow_inspections,"limits":limit,
                "excluded_tools":p.config.excluded_tools,"deny_globs":p.config.deny_globs,
                "languages":p.config.languages.iter().map(|l|json!({"id":l.id,"extensions":l.extensions})).collect::<Vec<_>>(),
                "command_profiles":p.config.commands.keys().collect::<Vec<_>>(),"debug_profiles":p.config.debuggers.keys().collect::<Vec<_>>(),"inspection_profiles":p.config.inspections.iter().map(|(name,profile)|json!({"name":name,"provider":profile.provider})).collect::<Vec<_>>()}),
            ),
            "initial_instructions" => Ok(json!({"instructions":INSTRUCTIONS})),
            "onboarding" => {
                let files = search::files(&p.root, limit.max_files)?;
                let recognized = [
                    "Cargo.toml",
                    "package.json",
                    "pyproject.toml",
                    "AGENTS.md",
                    "README.md",
                    "CMakeLists.txt",
                    "go.mod",
                ];
                let evidence=recognized.iter().filter_map(|path|p.root.read(path).ok().map(|body|json!({"path":path,"sha256":sha256(body.as_bytes()),"content":body}))).collect::<Vec<_>>();
                Ok(
                    json!({"files":files,"evidence":evidence,"memories":self.memories.list(&p.id)?,
                    "guidance":"Read evidence; record architecture, conventions and verified build commands in separate Markdown memories. Do not claim any test ran during onboarding."}),
                )
            }
            "list_dir" => {
                let listing = search::files(&p.root, limit.max_files)?;
                let prefix = optional_string(a, "relative_path").unwrap_or(".");
                p.root.resolve(prefix, false)?;
                let normalized = prefix.trim_end_matches('/');
                let recursive = boolean(a, "recursive", true);
                let files = listing
                    .files
                    .into_iter()
                    .filter(|path| {
                        let rel = if normalized == "." {
                            Some(path.as_str())
                        } else {
                            path.strip_prefix(&format!("{normalized}/"))
                        };
                        rel.is_some_and(|rel| recursive || !rel.contains('/'))
                    })
                    .collect::<Vec<_>>();
                Ok(json!({"files":files,"truncated":listing.truncated,"kind":"files_only"}))
            }
            "find_file" => {
                let mask = globset::Glob::new(string(a, "file_mask")?)
                    .map_err(|e| Fault::invalid(e.to_string()))?
                    .compile_matcher();
                let listing = search::files(&p.root, limit.max_files)?;
                Ok(
                    json!({"files":listing.files.into_iter().filter(|path|mask.is_match(path)).collect::<Vec<_>>(),"truncated":listing.truncated}),
                )
            }
            "read_file" => {
                let path = string(a, "relative_path")?;
                let text = p.root.read(path)?;
                let hash = sha256(text.as_bytes());
                let start = number(a, "start_line", 1) as usize;
                let lines = text.split_inclusive('\n').collect::<Vec<_>>();
                let end = number(a, "end_line", lines.len() as u64) as usize;
                if start == 0 || start > lines.len() + 1 || end > lines.len() || end + 1 < start {
                    return Err(Fault::invalid("Invalid inclusive line range"));
                }
                let content = if end < start {
                    String::new()
                } else {
                    lines[start - 1..end].concat()
                };
                Ok(
                    json!({"relative_path":path,"sha256":hash,"content":content,"start_line":start,"end_line":end,"total_lines":lines.len(),"line_base":1}),
                )
            }
            "search_for_pattern" => search::pattern(
                &p.root,
                string(a, "pattern")?,
                limit.max_files,
                limit.max_matches,
            ),
            "index_build" => p.store.index(&p.root, limit.max_files),
            "index_query" => {
                p.store.refresh(&p.root, limit.max_files)?;
                p.store.query(
                    &p.root,
                    string(a, "query")?,
                    number(a, "limit", 20) as usize,
                )
            }
            "get_symbols_overview" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let mut options = a.clone();
                options["query"] = json!("");
                let filter = symbols::Filter::new(&options, limit.max_matches)?;
                let found = client
                    .symbols(path)
                    .await
                    .map_err(backend_error)?
                    .into_iter()
                    .filter(|symbol| filter.accepts(symbol))
                    .collect::<Vec<_>>();
                let has_next = found.len() > filter.offset.saturating_add(filter.limit);
                let page = found
                    .into_iter()
                    .skip(filter.offset)
                    .take(filter.limit)
                    .collect::<Vec<_>>();
                let mut result = json!({"symbols":page,"backend":"lsp","language":client.language.id,"offset":filter.offset,"truncated":has_next,
                    "next_offset":if has_next{Some(filter.offset+filter.limit)}else{None}});
                if boolean(a, "group_by_file", false) {
                    result["groups"] = json!({path:result["symbols"]});
                }
                Ok(result)
            }
            "find_symbol" => self.find_symbols(p, a).await,
            "completion" | "completion_resolve" | "hover" | "signature_help"
            | "workspace_symbols" | "call_hierarchy" | "format_document" | "inlay_hints"
            | "inlay_hint_resolve" | "semantic_tokens" => ide::dispatch(self, p, tool, a).await,
            "list_inspection_providers" | "run_inspections" | "prepare_inspection_fix" => {
                inspections::dispatch(self, p, tool, a).await
            }
            "find_referencing_symbols" | "find_implementations" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let symbol = client
                    .symbol(path, string(a, "name_path")?)
                    .await
                    .map_err(backend_error)?;
                let kind = if tool == "find_referencing_symbols" {
                    "references"
                } else {
                    "implementation"
                };
                let locations = client
                    .at(path, symbol.selection_range.start, kind)
                    .await
                    .map_err(backend_error)?;
                self.enrich_locations(p, locations).await
            }
            "find_declaration" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let position: Position = serde_json::from_value(a["position"].clone())?;
                let kind = if boolean(a, "declaration_only", false) {
                    "declaration"
                } else {
                    "definition"
                };
                self.enrich_locations(
                    p,
                    client
                        .at(path, position, kind)
                        .await
                        .map_err(backend_error)?,
                )
                .await
            }
            "get_type_hierarchy" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let symbol = client
                    .symbol(path, string(a, "name_path")?)
                    .await
                    .map_err(backend_error)?;
                client
                    .hierarchy(path, symbol.selection_range.start, string(a, "direction")?)
                    .await
                    .map_err(backend_error)
            }
            "get_diagnostics_for_file" | "get_diagnostics_for_symbol" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let mut report = client.diagnostics(path).await.map_err(backend_error)?;
                if tool == "get_diagnostics_for_symbol" && report["available"] == true {
                    let symbol = client
                        .symbol(path, string(a, "name_path")?)
                        .await
                        .map_err(backend_error)?;
                    for key in ["items", "diagnostics"] {
                        if let Some(items) = report["report"][key].as_array_mut() {
                            items.retain(|item| {
                                serde_json::from_value::<Range>(item["range"].clone()).is_ok_and(
                                    |r| r.start < symbol.range.end && symbol.range.start <= r.end,
                                )
                            });
                        }
                    }
                    if let Some(findings) = report["findings"].as_array_mut() {
                        findings.retain(|finding| {
                            serde_json::from_value::<Range>(finding["range"].clone()).is_ok_and(
                                |range| {
                                    range.start < symbol.range.end
                                        && symbol.range.start <= range.end
                                },
                            )
                        });
                        let counts = (1..=4)
                            .map(|severity| {
                                findings
                                    .iter()
                                    .filter(|finding| finding["lsp_severity"] == severity)
                                    .count()
                            })
                            .collect::<Vec<_>>();
                        let total = findings.len();
                        let unknown = total - counts.iter().sum::<usize>();
                        let status = if report["freshness"] != "current" {
                            report["status"].clone()
                        } else if counts[0] > 0 {
                            json!("errors")
                        } else if counts[1] > 0 {
                            json!("warnings")
                        } else if total > 0 {
                            json!("findings")
                        } else {
                            json!("clean")
                        };
                        for (key, count) in ["errors", "warnings", "information", "hints"]
                            .into_iter()
                            .zip(counts)
                        {
                            report["summary"][key] = json!(count);
                        }
                        report["summary"]["total"] = json!(total);
                        report["summary"]["unknown"] = json!(unknown);
                        report["status"] = status;
                    }
                    report["summary"]["scope"] = json!("symbol");
                    report["scope"] = json!({"relative_path":path,"name_path":string(a,"name_path")?,"range":symbol.range});
                    report["name_path"] = symbol.name_path.into();
                }
                Ok(report)
            }
            "code_actions" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let range: Range = serde_json::from_value(a["range"].clone())?;
                Ok(
                    json!({"actions":client.actions(path,range).await.map_err(backend_error)?,"executed":false}),
                )
            }
            "prepare_code_action" | "move_symbol" | "inline_symbol" | "apply_quick_fix" => {
                refactor::prepare(self, p, tool, a).await
            }
            "restart_language_server" => {
                let warnings = self.stop_language_servers(&p.id).await;
                Ok(
                    json!({"stopped":true,"shutdown_warnings":warnings,"restart":"lazy on next semantic request"}),
                )
            }
            "transaction_prepare" => p.transactions.prepare(
                serde_json::from_value(a["changes"].clone())?,
                optional_string(a, "request_id"),
            ),
            "transaction_preview" => p.transactions.preview(string(a, "transaction_id")?),
            "transaction_list" => p.transactions.list(),
            "transaction_apply" | "transaction_rollback" => {
                let mut result = if tool == "transaction_apply" {
                    p.transactions.apply(string(a, "transaction_id")?)?
                } else {
                    p.transactions.rollback(string(a, "transaction_id")?)?
                };
                self.refresh_backends(p, &mut result).await?;
                Ok(result)
            }
            "transaction_recover" => {
                let result = p.transactions.recover()?;
                self.stop_language_servers(&p.id).await;
                Ok(result)
            }
            "create_text_file" => p.transactions.prepare(
                vec![Change {
                    path: string(a, "relative_path")?.into(),
                    expected_sha256: optional_string(a, "expected_sha256").map(str::to_string),
                    content: Some(string(a, "content")?.into()),
                }],
                optional_string(a, "request_id"),
            ),
            "replace_content" => {
                let path = string(a, "relative_path")?;
                let before = read_expected(p, path, string(a, "expected_sha256")?)?;
                let (after, count) = replace(
                    &before,
                    string(a, "needle")?,
                    string(a, "replacement")?,
                    boolean(a, "regex", false),
                    (
                        limit.max_file_bytes,
                        limit.max_matches,
                        number(a, "expected_matches", 1) as usize,
                    ),
                )?;
                if count != number(a, "expected_matches", 1) as usize {
                    return Err(Fault::conflict(format!(
                        "Expected {} matches, found {count}",
                        number(a, "expected_matches", 1)
                    )));
                }
                p.transactions.prepare(
                    vec![Change {
                        path: path.into(),
                        expected_sha256: Some(sha256(before.as_bytes())),
                        content: Some(after),
                    }],
                    None,
                )
            }
            "replacement_preview" => {
                occurrences::prepare(p, a, true, limit.max_matches, limit.max_output_bytes)
            }
            "replace_in_files" => {
                occurrences::prepare(p, a, false, limit.max_matches, limit.max_output_bytes)
            }
            "replace_lines" | "delete_lines" | "insert_at_line" => {
                let path = string(a, "relative_path")?;
                let before = read_expected(p, path, string(a, "expected_sha256")?)?;
                let start = number(a, "start_line", 1) as usize;
                let end = number(a, "end_line", start as u64) as usize;
                let after = line_edit(
                    &before,
                    start,
                    end,
                    optional_string(a, "content").unwrap_or(""),
                    tool == "insert_at_line",
                )?;
                p.transactions.prepare(
                    vec![Change {
                        path: path.into(),
                        expected_sha256: Some(sha256(before.as_bytes())),
                        content: Some(after),
                    }],
                    None,
                )
            }
            "replace_symbol_body"
            | "insert_before_symbol"
            | "insert_after_symbol"
            | "safe_delete_symbol" => {
                let path = string(a, "relative_path")?;
                let before = read_expected(p, path, string(a, "expected_sha256")?)?;
                let client = self.lsp_for(p, path).await?;
                let symbol = client
                    .symbol(path, string(a, "name_path")?)
                    .await
                    .map_err(backend_error)?;
                if p.root.read(path)? != before {
                    return Err(Fault::conflict("File changed while resolving symbol"));
                }
                if tool == "safe_delete_symbol" {
                    let references = client
                        .at(path, symbol.selection_range.start, "references")
                        .await
                        .map_err(backend_error)?;
                    if !references.is_null() && !references.as_array().is_some_and(Vec::is_empty) {
                        return Err(Fault::conflict("Symbol still has static references"));
                    }
                }
                let range = match tool {
                    "insert_before_symbol" => Range {
                        start: symbol.range.start,
                        end: symbol.range.start,
                    },
                    "insert_after_symbol" => Range {
                        start: symbol.range.end,
                        end: symbol.range.end,
                    },
                    _ => symbol.range,
                };
                let after = edit(
                    &before,
                    &[TextEdit {
                        range,
                        new_text: optional_string(a, "body").unwrap_or("").into(),
                    }],
                )
                .map_err(backend_error)?;
                let mut result = p.transactions.prepare(
                    vec![Change {
                        path: path.into(),
                        expected_sha256: Some(sha256(before.as_bytes())),
                        content: Some(after),
                    }],
                    None,
                )?;
                if tool == "safe_delete_symbol" {
                    result["safety_scope"] = json!(
                        "static_lsp_references_only; reflection/dynamic consumers may not be visible"
                    );
                }
                Ok(result)
            }
            "rename_symbol" => {
                let path = string(a, "relative_path")?;
                let target_before = p.root.read(path)?;
                let client = self.lsp_for(p, path).await?;
                let symbol = client
                    .symbol(path, string(a, "name_path")?)
                    .await
                    .map_err(backend_error)?;
                let listing = search::files(&p.root, limit.max_files)?;
                if listing.truncated {
                    return Err(Fault::new(
                        "LIMIT_EXCEEDED",
                        "Rename snapshot would omit files",
                    ));
                }
                let mut snapshots = BTreeMap::new();
                let mut bytes = 0;
                for file in listing.files {
                    if let Ok(text) = p.root.read(&file) {
                        bytes += text.len();
                        if bytes > 64 * 1024 * 1024 {
                            return Err(Fault::new(
                                "LIMIT_EXCEEDED",
                                "Native rename snapshot exceeds 64 MiB",
                            ));
                        }
                        snapshots.insert(file, text);
                    }
                }
                if snapshots.get(path) != Some(&target_before) {
                    return Err(Fault::conflict(
                        "Target changed while locating the rename symbol",
                    ));
                }
                let workspace_edit = client
                    .rename(
                        path,
                        symbol.selection_range.start,
                        string(a, "new_name")?,
                        &sha256(target_before.as_bytes()),
                    )
                    .await
                    .map_err(backend_error)?;
                let changes = client
                    .changes(&workspace_edit, &snapshots)
                    .await
                    .map_err(backend_error)?;
                p.transactions.prepare(changes, None)
            }
            "list_memories" => self.memories.list(&p.id),
            "read_memory" => self.memories.read(&p.id, string(a, "name")?),
            "write_memory" => self.memories.write(
                &p.id,
                string(a, "name")?,
                Some(string(a, "content")?.into()),
                optional_string(a, "expected_sha256").map(str::to_string),
            ),
            "delete_memory" => self.memories.write(
                &p.id,
                string(a, "name")?,
                None,
                Some(string(a, "expected_sha256")?.into()),
            ),
            "edit_memory" => {
                let name = string(a, "name")?;
                let memory = self.memories.read(&p.id, name)?;
                if memory["sha256"] != string(a, "expected_sha256")? {
                    return Err(Fault::conflict("Memory version mismatch"));
                }
                let (after, count) = replace(
                    memory["content"]
                        .as_str()
                        .ok_or_else(|| Fault::invalid("Memory body missing"))?,
                    string(a, "needle")?,
                    string(a, "replacement")?,
                    false,
                    (
                        limit.max_file_bytes,
                        limit.max_matches,
                        number(a, "expected_matches", 1) as usize,
                    ),
                )?;
                if count != number(a, "expected_matches", 1) as usize {
                    return Err(Fault::conflict("Unexpected replacement count"));
                }
                self.memories.write(
                    &p.id,
                    name,
                    Some(after),
                    Some(string(a, "expected_sha256")?.into()),
                )
            }
            "rename_memory" => self.memories.rename(
                &p.id,
                string(a, "old_name")?,
                string(a, "new_name")?,
                string(a, "expected_sha256")?,
            ),
            "memory_export" => Ok(json!({"bundle":self.memories.export(&p.id)?})),
            "memory_import" => self.memories.import(&p.id, &a["bundle"]),
            "memory_move_scope" => self.memories.move_scope(
                &p.id,
                string(a, "name")?,
                string(a, "destination_project")?,
                string(a, "destination_name")?,
                string(a, "expected_sha256")?,
            ),
            "memory_transaction_preview" => {
                self.memories.preview(&p.id, string(a, "transaction_id")?)
            }
            "memory_transaction_apply" => self.memories.apply(&p.id, string(a, "transaction_id")?),
            "check_memories" => self.memories.check(&p.id),
            "search_memories" => {
                self.memories
                    .search(&p.id, string(a, "pattern")?, limit.max_matches)
            }
            "recover_memories" => {
                if self.projects.values().any(|p| !p.config.allow_write)
                    || !self.config.allow_global_memory_write
                {
                    return Err(Fault::denied(
                        "Global memory journal recovery is operator-only unless all configured scopes are writable",
                    ));
                }
                self.memories.recover()
            }
            "execute_shell_command" => {
                let config = p
                    .config
                    .commands
                    .get(string(a, "profile")?)
                    .ok_or_else(|| Fault::denied("Unknown command profile"))?;
                if let Some(sandbox) = &p.config.sandbox {
                    shell::run_with_sandbox(
                        config,
                        p.root.root(),
                        limit.max_output_bytes,
                        Some((sandbox, p.state.as_path())),
                    )
                    .await
                } else {
                    shell::run(config, p.root.root(), limit.max_output_bytes).await
                }
            }
            "debug_start" => {
                if self.debuggers.lock().await.contains_key(&p.id) {
                    return Err(Fault::conflict("Debugger already running; stop it first"));
                }
                let profile = string(a, "profile")?;
                let config = p
                    .config
                    .debuggers
                    .get(profile)
                    .ok_or_else(|| Fault::denied("Unknown debugger profile"))?;
                let adapter = DebugAdapter::start_with_sandbox(
                    config,
                    &p.root,
                    p.config
                        .sandbox
                        .as_ref()
                        .map(|policy| (policy, p.state.as_path())),
                )
                .await
                .map_err(backend_error)?;
                let capabilities = adapter.capabilities.clone();
                let session_id = uuid::Uuid::new_v4().to_string();
                self.debuggers.lock().await.insert(
                    p.id.clone(),
                    DebugSession {
                        id: session_id.clone(),
                        profile: profile.into(),
                        adapter,
                    },
                );
                Ok(
                    json!({"started":true,"session_id":session_id,"capabilities":capabilities,"debuggee_launched":false,"unrestricted_host_debug":!p.config.debug_launches.contains_key(profile),"sandbox_requested":p.config.sandbox.is_some()}),
                )
            }
            "debug_request" | "debug_events" | "debug_stop" | "debug_batch" => {
                let (session_id, profile, adapter) = {
                    let debuggers = self.debuggers.lock().await;
                    let session = debuggers
                        .get(&p.id)
                        .ok_or_else(|| Fault::new("SESSION_CLOSED", "No active debug adapter"))?;
                    (
                        session.id.clone(),
                        session.profile.clone(),
                        session.adapter.clone(),
                    )
                };
                if optional_string(a, "session_id").is_some_and(|id| id != session_id) {
                    return Err(Fault::denied(
                        "Debug session does not belong to this active project session",
                    ));
                }
                if adapter.is_closed() && tool != "debug_stop" {
                    return Err(Fault::new("SESSION_CLOSED", "Debug adapter has exited"));
                }
                if tool == "debug_stop" {
                    let response = adapter
                        .request("disconnect", json!({"terminateDebuggee":true}))
                        .await;
                    let cleanup = adapter.shutdown().await;
                    self.debuggers.lock().await.remove(&p.id);
                    Ok(
                        json!({"adapter_dropped":true,"disconnect_succeeded":response.is_ok(),"cleanup_succeeded":cleanup.is_ok(),"warning":response.err().map(|e|e.to_string()),"cleanup_warning":cleanup.err().map(|e|e.to_string())}),
                    )
                } else if tool == "debug_events" {
                    let mut result = adapter.poll().await.map_err(backend_error)?;
                    result["unrestricted_host_debug"] =
                        json!(!p.config.debug_launches.contains_key(&profile));
                    Ok(result)
                } else if tool == "debug_batch" {
                    let commands = a["commands"]
                        .as_array()
                        .ok_or_else(|| Fault::invalid("commands must be an array"))?;
                    if commands.is_empty() || commands.len() > 32 {
                        return Err(Fault::invalid("Debug batches require 1..32 commands"));
                    }
                    let mut outcomes = Vec::new();
                    let mut successful = true;
                    let deadline =
                        tokio::time::Instant::now() + Duration::from_secs(limit.rpc_timeout_secs);
                    for command in commands {
                        let request_id = uuid::Uuid::new_v4().to_string();
                        let name = string(command, "command")?;
                        let arguments = match p.config.debug_arguments(
                            &profile,
                            name,
                            &command["arguments"],
                        ) {
                            Ok(arguments) => arguments,
                            Err(error) => {
                                successful = false;
                                outcomes.push(json!({"request_id":request_id,"command":name,"ok":false,"error":error}));
                                if boolean(a, "stop_on_error", true) {
                                    break;
                                } else {
                                    continue;
                                }
                            }
                        };
                        let result = match tokio::time::timeout_at(
                            deadline,
                            adapter.request(name, arguments),
                        )
                        .await
                        {
                            Ok(result) => result,
                            Err(_) => {
                                successful = false;
                                outcomes.push(json!({"request_id":request_id,"command":name,"ok":false,"error":{"code":"TIMEOUT","message":"Batch deadline elapsed; adapter-side outcome is unknown"}}));
                                break;
                            }
                        };
                        let ok = result.is_ok();
                        successful &= ok;
                        outcomes.push(match result{Ok(value)=>json!({"request_id":request_id,"command":name,"ok":true,"result":value}),
                            Err(error)=>json!({"request_id":request_id,"command":name,"ok":false,"error":error.to_string()})});
                        if !ok && boolean(a, "stop_on_error", true) {
                            break;
                        }
                    }
                    Ok(
                        json!({"session_id":session_id,"success":successful,"steps_executed":outcomes.len(),"outcomes":outcomes,"closed":adapter.is_closed(),"language":"odin-dap-batch","sandbox_requested":p.config.sandbox.is_some(),"unrestricted_host_debug":!p.config.debug_launches.contains_key(&profile)}),
                    )
                } else {
                    let command = string(a, "command")?;
                    if command == "cancel_ticket" {
                        adapter
                            .cancel(string(&a["arguments"], "ticket")?)
                            .await
                            .map_err(backend_error)
                    } else {
                        let arguments =
                            p.config
                                .debug_arguments(&profile, command, &a["arguments"])?;
                        adapter
                            .request(command, arguments)
                            .await
                            .map_err(backend_error)
                    }
                }
            }
            "result_read" => results::read(self, p, a),
            "audit_log" => p.store.recent(number(a, "limit", 50) as usize),
            _ => Err(Fault::new(
                "UNKNOWN_TOOL",
                format!("No dispatcher for {tool}"),
            )),
        }
    }
    async fn find_symbols(&self, p: &Project, a: &Value) -> Result<Value> {
        if p.config.languages.is_empty() {
            return Err(Fault::new(
                "MISSING_BACKEND",
                "Symbol search requires a host-configured language server",
            ));
        }
        let query_filter = symbols::Filter::new(a, self.config.limits.max_matches)?;
        let filter = optional_string(a, "relative_path");
        if let Some(filter) = filter {
            p.root.resolve(filter, false)?;
        }
        let listing = search::files(&p.root, self.config.limits.max_files)?;
        let mut symbols = Vec::new();
        let mut errors = Vec::new();
        let mut truncated = listing.truncated;
        let mut unavailable_languages = BTreeSet::new();
        let mut successful_documents = 0;
        let mut skipped = 0usize;
        let mut has_next = false;
        let mut page_left = (16 * 1024 * 1024usize - 1024)
            / if boolean(a, "group_by_file", false) {
                2
            } else {
                1
            };
        for path in listing.files {
            if filter.is_some_and(|f| {
                f != "." && path != f && !path.starts_with(&format!("{}/", f.trim_end_matches('/')))
            }) {
                continue;
            }
            let language = match self.language_for(p, &path) {
                Ok(l) => l,
                Err(_) => continue,
            };
            if unavailable_languages.contains(&language.id) {
                continue;
            }
            let client = match self.lsp_for(p, &path).await {
                Ok(c) => c,
                Err(e) => {
                    unavailable_languages.insert(language.id);
                    errors.push(json!({"path":path,"error":e}));
                    if errors.len() >= 20 {
                        truncated = true;
                        break;
                    }
                    continue;
                }
            };
            // Retain one source snapshot per RPC, not one clone per symbol.
            // Old backend ranges must never be applied to a newer file body.
            let body_snapshot = if boolean(a, "include_body", false) {
                Some(p.root.read(&path)?)
            } else {
                None
            };
            match client.symbols(&path).await {
                Ok(found) => {
                    if body_snapshot
                        .as_ref()
                        .is_some_and(|before| p.root.read(&path).as_ref().ok() != Some(before))
                    {
                        return Err(Fault::conflict(
                            "Source changed while resolving symbol bodies",
                        ));
                    }
                    successful_documents += 1;
                    for symbol in found {
                        if !query_filter.accepts(&symbol) {
                            continue;
                        }
                        if skipped < query_filter.offset {
                            skipped += 1;
                            continue;
                        }
                        if symbols.len() >= query_filter.limit {
                            has_next = true;
                            break;
                        }
                        let mut selected_body = None;
                        if let Some(body) = &body_snapshot {
                            let start = offset(body, symbol.range.start).map_err(backend_error)?;
                            let end = offset(body, symbol.range.end).map_err(backend_error)?;
                            selected_body = Some(body.get(start..end).ok_or_else(|| {
                                Fault::new(
                                    "BACKEND_ERROR",
                                    "Backend symbol range is reversed or outside the document",
                                )
                            })?);
                        }
                        let cost = symbols::value_cost(&symbol, selected_body, page_left)?;
                        page_left -= cost;
                        let mut value = serde_json::to_value(&symbol)?;
                        if let Some(body) = selected_body {
                            value["body"] = body.into();
                        }
                        if let Some(body) = &body_snapshot {
                            value["sha256"] = sha256(body.as_bytes()).into();
                        }
                        symbols.push(value);
                    }
                }
                Err(error) => {
                    if error.to_string().starts_with("CONFLICT") {
                        return Err(backend_error(error));
                    }
                    errors.push(json!({"path":path,"error":error.to_string()}));
                    if errors.len() >= 20 {
                        truncated = true;
                        break;
                    }
                }
            }
            if has_next {
                break;
            }
        }
        if successful_documents == 0 {
            return Err(Fault::new(
                "CAPABILITY_UNAVAILABLE",
                if errors.is_empty() {
                    "No files in this scope have a configured semantic backend".into()
                } else {
                    format!(
                        "No semantic backend completed a symbol query: {}",
                        serde_json::to_string(&errors)?
                    )
                },
            ));
        }
        let page = symbols;
        let mut result = json!({"symbols":page,"truncated":truncated||has_next,"backend_errors":errors,"source":"lsp_not_regex","offset":query_filter.offset,
            "next_offset":if has_next{Some(query_filter.offset+query_filter.limit)}else{None},"successful_documents":successful_documents});
        if boolean(a, "group_by_file", false) {
            let mut groups = BTreeMap::<String, Vec<Value>>::new();
            for item in result["symbols"].as_array().into_iter().flatten() {
                if let Some(path) = item["relative_path"].as_str() {
                    groups.entry(path.into()).or_default().push(item.clone());
                }
            }
            result["groups"] = serde_json::to_value(groups)?;
        }
        Ok(result)
    }
    async fn enrich_locations(&self, p: &Project, raw: Value) -> Result<Value> {
        let locations = if raw.is_null() {
            Vec::new()
        } else if let Some(array) = raw.as_array() {
            array.clone()
        } else {
            vec![raw]
        };
        let truncated = locations.len() > self.config.limits.max_matches;
        let mut out = Vec::new();
        for location in locations.into_iter().take(self.config.limits.max_matches) {
            let uri = location
                .get("uri")
                .or_else(|| location.get("targetUri"))
                .and_then(Value::as_str);
            let range_value = location
                .get("range")
                .or_else(|| location.get("targetSelectionRange"));
            let mut value = json!({"location":location,"external":true});
            if let (Some(uri), Some(range)) = (uri, range_value)
                && let Ok(url) = url::Url::parse(uri)
                && url.query().is_none()
                && url.fragment().is_none()
                && let Ok(absolute) = url.to_file_path()
                && let Ok(absolute) = absolute.canonicalize()
                && let Ok(path) = p.root.relative(&absolute)
            {
                value["external"] = false.into();
                value["relative_path"] = path.clone().into();
                if let Ok(range) = serde_json::from_value::<Range>(range.clone())
                    && let Ok(client) = self.lsp_for(p, &path).await
                    && let Ok(symbols) = client.symbols(&path).await
                {
                    let enclosing = symbols
                        .into_iter()
                        .filter(|s| s.range.contains(range.start))
                        .min_by_key(|s| {
                            (
                                s.range.end.line.saturating_sub(s.range.start.line),
                                s.range
                                    .end
                                    .character
                                    .saturating_sub(s.range.start.character),
                            )
                        });
                    value["enclosing_symbol"] = serde_json::to_value(enclosing)?;
                }
            }
            out.push(value);
        }
        Ok(json!({"locations":out,"truncated":truncated}))
    }
}
pub fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Fault::invalid(format!("Expected string argument: {key}")))
}
pub fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}
pub fn number(value: &Value, key: &str, default: u64) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(default)
}
pub fn boolean(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}
fn backend_error(error: anyhow::Error) -> Fault {
    let message = error.to_string();
    let code = [
        "CAPABILITY_UNAVAILABLE",
        "CONFLICT",
        "LIMIT_EXCEEDED",
        "SANDBOX_UNAVAILABLE",
        "SANDBOX_REQUIRED",
    ]
    .into_iter()
    .find(|code| message.starts_with(code))
    .unwrap_or("BACKEND_ERROR");
    Fault::new(code, message)
}
fn read_expected(p: &Project, path: &str, expected: &str) -> Result<String> {
    let content = p.root.read(path)?;
    if sha256(content.as_bytes()) != expected {
        return Err(Fault::conflict("File hash does not match expected_sha256"));
    }
    Ok(content)
}
fn replace(
    text: &str,
    needle: &str,
    replacement: &str,
    regex: bool,
    limits: (usize, usize, usize),
) -> Result<(String, usize)> {
    text_replace::replace(text, needle, replacement, regex, limits)
}
fn line_edit(text: &str, start: usize, end: usize, content: &str, insert: bool) -> Result<String> {
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    if start == 0 || start > lines.len() + 1 || (!insert && (end < start || end > lines.len())) {
        return Err(Fault::invalid("Invalid line edit range"));
    }
    let begin = lines.iter().take(start - 1).map(|s| s.len()).sum::<usize>();
    let finish = if insert {
        begin
    } else {
        lines.iter().take(end).map(|s| s.len()).sum()
    };
    let mut result = text.to_string();
    result.replace_range(begin..finish, content);
    Ok(result)
}
pub const INSTRUCTIONS: &str = "Odin is a local Rust code-intelligence service. Every call is scoped to a configured project ID; there is no process-global active project. Start with get_symbols_overview/find_symbol; read bodies only when needed. LSP positions use zero-based lines and UTF-16 columns; read_file/line edits use one-based inclusive lines. Read the SHA-256 first. All native source edits return an immutable transaction preview; use transaction_apply only after reviewing it. Successful application is NOT a passing build: run an authorized build/test profile and inspect its actual exit status. Native JSON flow is a bounded dataflow, not Python eval. This standalone build has no external tool bridge. Semantic operations connect directly to configured language servers; debugging connects directly to configured DAP adapters. Missing features have no hidden fallback. Missing capabilities are errors, never fabricated successful results.";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_replacement_count() {
        let (s, n) = replace("a a", "a", "b", false, (1024, 10, 2)).unwrap();
        assert_eq!((s, n), ("b b".into(), 2));
    }
    #[test]
    fn regex_replacement_captures() {
        assert_eq!(
            replace("foo12", r"foo(\d+)", "bar$1", true, (1024, 10, 1))
                .unwrap()
                .0,
            "bar12"
        );
    }
    #[test]
    fn preserves_crlf_line_edit() {
        assert_eq!(
            line_edit("a\r\nb\r\n", 2, 2, "c\r\n", false).unwrap(),
            "a\r\nc\r\n"
        );
    }
    #[test]
    fn inserts_into_empty_document() {
        assert_eq!(line_edit("", 1, 1, "hello", true).unwrap(), "hello");
    }
}
