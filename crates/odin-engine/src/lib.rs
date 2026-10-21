#![forbid(unsafe_code)]
//! Shared application service for CLI, MCP, and the read-only dashboard.
mod dispatch;
pub mod flow;
mod formatter;
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
    transaction::Transactions,
};
use odin_protocol::{
    dap::DebugAdapter,
    lsp::LspClient,
    text::{Range, offset},
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
        let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
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
        match tool {
            "status" => self.dispatch_meta(p, tool, a).await,
            "get_version" => self.dispatch_meta(p, tool, a).await,
            "get_schemas" => self.dispatch_meta(p, tool, a).await,
            "get_capabilities" => self.dispatch_meta(p, tool, a).await,
            "list_projects" => self.dispatch_meta(p, tool, a).await,
            "get_current_config" => self.dispatch_meta(p, tool, a).await,
            "initial_instructions" => self.dispatch_meta(p, tool, a).await,
            "onboarding" => self.dispatch_meta(p, tool, a).await,
            "list_dir" => self.dispatch_files(p, tool, a).await,
            "find_file" => self.dispatch_files(p, tool, a).await,
            "read_file" => self.dispatch_files(p, tool, a).await,
            "search_for_pattern" => self.dispatch_files(p, tool, a).await,
            "index_build" => self.dispatch_files(p, tool, a).await,
            "index_query" => self.dispatch_files(p, tool, a).await,
            "get_symbols_overview" => self.dispatch_symbols(p, tool, a).await,
            "find_symbol" => self.dispatch_symbols(p, tool, a).await,
            "completion" | "completion_resolve" | "hover" | "signature_help"
            | "workspace_symbols" | "call_hierarchy" | "format_document" | "inlay_hints"
            | "inlay_hint_resolve" | "semantic_tokens" => self.dispatch_semantic(p, tool, a).await,
            "list_inspection_providers" | "run_inspections" | "prepare_inspection_fix" => {
                self.dispatch_semantic(p, tool, a).await
            }
            "find_referencing_symbols" | "find_implementations" => {
                self.dispatch_symbols(p, tool, a).await
            }
            "find_declaration" => self.dispatch_symbols(p, tool, a).await,
            "get_type_hierarchy" => self.dispatch_symbols(p, tool, a).await,
            "get_diagnostics_for_file" | "get_diagnostics_for_symbol" => {
                self.dispatch_symbols(p, tool, a).await
            }
            "code_actions" => self.dispatch_symbols(p, tool, a).await,
            "prepare_code_action" | "move_symbol" | "inline_symbol" | "apply_quick_fix" => {
                self.dispatch_semantic(p, tool, a).await
            }
            "restart_language_server" => self.dispatch_symbols(p, tool, a).await,
            "transaction_prepare" => self.dispatch_transactions(p, tool, a).await,
            "transaction_preview" => self.dispatch_transactions(p, tool, a).await,
            "transaction_list" => self.dispatch_transactions(p, tool, a).await,
            "transaction_apply" | "transaction_rollback" => {
                self.dispatch_transactions(p, tool, a).await
            }
            "transaction_recover" => self.dispatch_transactions(p, tool, a).await,
            "create_text_file" => self.dispatch_edits(p, tool, a).await,
            "replace_content" => self.dispatch_edits(p, tool, a).await,
            "replacement_preview" => self.dispatch_edits(p, tool, a).await,
            "replace_in_files" => self.dispatch_edits(p, tool, a).await,
            "replace_lines" | "delete_lines" | "insert_at_line" => {
                self.dispatch_edits(p, tool, a).await
            }
            "replace_symbol_body"
            | "insert_before_symbol"
            | "insert_after_symbol"
            | "safe_delete_symbol" => self.dispatch_edits(p, tool, a).await,
            "rename_symbol" => self.dispatch_edits(p, tool, a).await,
            "list_memories" => self.dispatch_memory(p, tool, a).await,
            "read_memory" => self.dispatch_memory(p, tool, a).await,
            "write_memory" => self.dispatch_memory(p, tool, a).await,
            "delete_memory" => self.dispatch_memory(p, tool, a).await,
            "edit_memory" => self.dispatch_memory(p, tool, a).await,
            "rename_memory" => self.dispatch_memory(p, tool, a).await,
            "memory_export" => self.dispatch_memory(p, tool, a).await,
            "memory_import" => self.dispatch_memory(p, tool, a).await,
            "memory_move_scope" => self.dispatch_memory(p, tool, a).await,
            "memory_transaction_preview" => self.dispatch_memory(p, tool, a).await,
            "memory_transaction_apply" => self.dispatch_memory(p, tool, a).await,
            "check_memories" => self.dispatch_memory(p, tool, a).await,
            "search_memories" => self.dispatch_memory(p, tool, a).await,
            "recover_memories" => self.dispatch_memory(p, tool, a).await,
            "execute_shell_command" => self.dispatch_exec(p, tool, a).await,
            "debug_start" => self.dispatch_exec(p, tool, a).await,
            "debug_request" | "debug_events" | "debug_stop" | "debug_batch" => {
                self.dispatch_exec(p, tool, a).await
            }
            "result_read" => self.dispatch_meta(p, tool, a).await,
            "audit_log" => self.dispatch_meta(p, tool, a).await,
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
/// Like [`number`], but saturates instead of truncating on 32-bit targets, so an
/// oversized caller value can never wrap into a small, valid-looking index.
pub fn number_usize(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map_or(default, saturating_usize)
}
/// Convert a `u64` to `usize`, saturating at `usize::MAX`.
pub fn saturating_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
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
    fn number_usize_reads_present_numbers_and_defaults() {
        let value = json!({"limit": 7, "text": "x", "negative": -1});
        assert_eq!(number_usize(&value, "limit", 20), 7);
        assert_eq!(number_usize(&value, "missing", 20), 20);
        assert_eq!(number_usize(&value, "text", 20), 20);
        assert_eq!(number_usize(&value, "negative", 20), 20);
    }
    #[test]
    fn number_usize_saturates_oversized_values() {
        let value = json!({"limit": u64::MAX});
        assert_eq!(number_usize(&value, "limit", 1), usize::MAX);
        let beyond_u32 = json!({"limit": u64::from(u32::MAX) + 1});
        assert_eq!(
            number_usize(&beyond_u32, "limit", 1),
            usize::try_from(u64::from(u32::MAX) + 1).unwrap_or(usize::MAX)
        );
    }
    #[test]
    fn saturating_usize_converts_within_range_and_saturates() {
        assert_eq!(saturating_usize(0), 0);
        assert_eq!(saturating_usize(42), 42);
        assert_eq!(saturating_usize(u64::MAX), usize::MAX);
    }
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
