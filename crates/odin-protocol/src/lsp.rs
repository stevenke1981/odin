use crate::{
    rpc::{Dialect, Framing, RpcProcess},
    text::{Position, Range, TextEdit},
    workspace::Planner,
};
use anyhow::{Result, anyhow, bail};
use odin_core::{config::LanguageConfig, paths::SafeRoot, sha256, transaction::Change};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    time::Duration,
};
use tokio::{sync::Mutex, time::Instant};
use url::Url;

const HIERARCHY_PREPARE_LIMIT: usize = 128;
const HIERARCHY_RELATION_LIMIT: usize = 4096;
const HIERARCHY_RANGE_LIMIT: usize = 8192;
const HIERARCHY_JSON_LIMIT: usize = 4 * 1024 * 1024;
const HIERARCHY_TIMEOUT: &str = "TIMEOUT: hierarchy query exceeded configured total deadline";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub name_path: String,
    pub kind: u64,
    pub relative_path: String,
    pub range: Range,
    pub selection_range: Range,
}
#[derive(Debug, Clone)]
struct OpenDocument {
    hash: String,
    version: i64,
    content: String,
}
struct DocumentSnapshot {
    path: String,
    uri: String,
    content: String,
    hash: String,
    version: i64,
}
impl DocumentSnapshot {
    fn metadata(&self) -> Value {
        json!({"path":self.path,"uri":self.uri,"sha256":self.hash,"version":self.version})
    }
}
pub struct LspClient {
    rpc: Arc<RpcProcess>,
    pub capabilities: Value,
    pub language: LanguageConfig,
    root: SafeRoot,
    documents: Mutex<BTreeMap<String, OpenDocument>>,
    shutting_down: AtomicBool,
    next_version: AtomicI64,
    timeout: Duration,
}
impl LspClient {
    pub async fn start(
        root: SafeRoot,
        language: LanguageConfig,
        timeout: Duration,
    ) -> Result<Arc<Self>> {
        Self::start_with_sandbox(root, language, timeout, None).await
    }
    pub async fn start_with_sandbox(
        root: SafeRoot,
        language: LanguageConfig,
        timeout: Duration,
        sandbox: Option<(
            &odin_core::sandbox_policy::MxcSandboxConfig,
            &std::path::Path,
        )>,
    ) -> Result<Arc<Self>> {
        let uri = Url::from_directory_path(root.root())
            .map_err(|_| anyhow!("Invalid workspace path"))?
            .to_string();
        let workspace = json!([{"uri":uri,"name":"odin-workspace"}]);
        // An isolated server cannot monitor the host process namespace. LSP
        // permits null; the native guard/job owns its complete lifetime.
        let process_id = sandbox.is_none().then(std::process::id);
        let rpc = RpcProcess::spawn_with_sandbox(
            &language.program,
            &language.args,
            root.root(),
            Framing::ContentLength,
            Dialect::JsonRpc,
            timeout,
            workspace.clone(),
            language.settings.clone(),
            sandbox,
        )?;
        let result=rpc.request("initialize",json!({
            "processId":process_id,"clientInfo":{"name":"odin","version":env!("CARGO_PKG_VERSION")},
            "rootUri":uri,"workspaceFolders":workspace,"initializationOptions":language.initialization_options,
            "capabilities":{
                "window":{"workDoneProgress":true},
                "experimental":{"serverStatusNotification":true},
                "general":{"positionEncodings":["utf-16"]},"offsetEncoding":["utf-16"],
                "workspace":{"configuration":true,"workspaceFolders":true,"applyEdit":false,
                    "symbol":{"dynamicRegistration":false},
                    "workspaceEdit":{"documentChanges":true,"resourceOperations":["create","rename","delete"]}},
                "textDocument":{"documentSymbol":{"hierarchicalDocumentSymbolSupport":true},
                    "synchronization":{"dynamicRegistration":false,"didSave":false},
                    "codeAction":{"dataSupport":true,"resolveSupport":{"properties":["edit"]}},
                    "completion":{"dynamicRegistration":false,"contextSupport":true,"completionItem":{
                        "snippetSupport":false,"documentationFormat":["plaintext","markdown"],
                        "resolveSupport":{"properties":["documentation","detail","additionalTextEdits"]}}},
                    "hover":{"dynamicRegistration":false,"contentFormat":["plaintext","markdown"]},
                    "signatureHelp":{"dynamicRegistration":false,"signatureInformation":{
                        "documentationFormat":["plaintext","markdown"],"parameterInformation":{"labelOffsetSupport":true}}},
                    "callHierarchy":{"dynamicRegistration":false},"typeHierarchy":{"dynamicRegistration":false},
                    "formatting":{"dynamicRegistration":false},"rangeFormatting":{"dynamicRegistration":false},
                    "inlayHint":{"dynamicRegistration":false,"resolveSupport":{"properties":["tooltip","textEdits","label.tooltip","label.location"]}},
                    "semanticTokens":{"dynamicRegistration":false,"requests":{"range":true,"full":true},
                        "tokenTypes":["namespace","type","class","enum","interface","struct","typeParameter","parameter","variable","property","enumMember","event","function","method","macro","keyword","modifier","comment","string","number","regexp","operator","decorator"],
                        "tokenModifiers":["declaration","definition","readonly","static","deprecated","abstract","async","modification","documentation","defaultLibrary"],
                        "formats":["relative"],"overlappingTokenSupport":false,"multilineTokenSupport":false},
                    "publishDiagnostics":{"versionSupport":true},"diagnostic":{"dynamicRegistration":false}}
            }
        })).await?;
        let capabilities = result["capabilities"].clone();
        if !capabilities.is_object() {
            bail!("Invalid LSP initialize capabilities");
        }
        for encoding in [
            capabilities.get("positionEncoding"),
            result.get("offsetEncoding"),
            capabilities.get("offsetEncoding"),
        ]
        .into_iter()
        .flatten()
        {
            if encoding != &Value::String("utf-16".into()) {
                bail!(
                    "CAPABILITY_UNAVAILABLE: language server selected a position encoding other than UTF-16"
                );
            }
        }
        rpc.notify("initialized", json!({})).await?;
        if !language.settings.is_null() {
            rpc.notify(
                "workspace/didChangeConfiguration",
                json!({"settings":language.settings}),
            )
            .await?;
        }
        Ok(Arc::new(Self {
            rpc,
            capabilities,
            language,
            root,
            documents: Mutex::new(BTreeMap::new()),
            shutting_down: AtomicBool::new(false),
            next_version: AtomicI64::new(1),
            timeout,
        }))
    }
    pub fn is_closed(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst) || self.rpc.is_closed()
    }
    pub fn status(&self) -> Result<Value> {
        let transport = self.rpc.status()?;
        let status = &transport["server_status"];
        let readiness = if self.is_closed() {
            "closed"
        } else if status["health"] == "error" {
            "error"
        } else if status["quiescent"] == true {
            "ready"
        } else if status["quiescent"] == false
            || transport["active_progress"]
                .as_object()
                .is_some_and(|p| !p.is_empty())
        {
            "pending"
        } else {
            "unknown"
        };
        Ok(
            json!({"initialized":!self.is_closed(),"readiness":readiness,"capabilities":self.capabilities,
            "transport":transport,"index_complete_proven":false}),
        )
    }
    pub async fn shutdown(&self) -> Result<()> {
        if self.shutting_down.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let outcome = async {
            self.rpc.request("shutdown", Value::Null).await?;
            self.rpc.notify("exit", Value::Null).await?;
            self.rpc.wait_for_exit().await
        }
        .await;
        self.documents.lock().await.clear();
        let cleanup = self.rpc.terminate().await;
        outcome?;
        cleanup
    }
    fn sync_options(&self) -> (bool, u64) {
        match &self.capabilities["textDocumentSync"] {
            Value::Number(n) => (true, n.as_u64().unwrap_or(0)),
            Value::Object(options) => (
                options
                    .get("openClose")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                options.get("change").and_then(Value::as_u64).unwrap_or(0),
            ),
            _ => (false, 0),
        }
    }
    fn version(&self) -> Result<i64> {
        self.next_version
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |version| {
                version.checked_add(1)
            })
            .map_err(|_| anyhow!("LSP document version exhausted; restart the server"))
    }
    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities
            .get(capability)
            .is_some_and(|v| v == &Value::Bool(true) || v.is_object())
    }
    pub fn matches(&self, path: &str) -> bool {
        std::path::Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| {
                self.language
                    .extensions
                    .iter()
                    .any(|e| e.eq_ignore_ascii_case(ext))
            })
    }
    pub fn uri(&self, path: &str) -> Result<String> {
        Ok(Url::from_file_path(self.root.resolve(path, false)?)
            .map_err(|_| anyhow!("Invalid file URI"))?
            .into())
    }
    pub fn relative_uri(&self, uri: &str) -> Result<String> {
        crate::workspace::relative_uri(&self.root, uri, false)
    }
    pub async fn sync(&self, path: &str) -> Result<String> {
        if self.is_closed() {
            bail!("Language server is closed; explicitly restart it");
        }
        let content = self.root.read(path)?;
        let hash = sha256(content.as_bytes());
        let uri = self.uri(path)?;
        let (open_close, change_kind) = self.sync_options();
        let mut documents = self.documents.lock().await;
        match documents.get_mut(path) {
            None => {
                let version = self.version()?;
                self.rpc.invalidate_diagnostics(&uri)?;
                if open_close {
                    self.rpc.notify("textDocument/didOpen",json!({"textDocument":{"uri":uri,"languageId":odin_core::languages::language_id_for_path(&self.language.id,std::path::Path::new(path)),"version":version,"text":content}})).await?;
                }
                documents.insert(
                    path.into(),
                    OpenDocument {
                        hash,
                        version,
                        content: content.clone(),
                    },
                );
            }
            Some(document) if document.hash != hash => {
                let version = self.version()?;
                self.rpc.invalidate_diagnostics(&uri)?;
                let changes = match change_kind {
                    0 => None,
                    1 => Some(json!([{"text":content}])),
                    2 => Some(
                        json!([{"range":{"start":{"line":0,"character":0},"end":crate::text::end_position(&document.content)},"text":content}]),
                    ),
                    _ => bail!("CAPABILITY_UNAVAILABLE: unsupported textDocumentSync kind"),
                };
                if let Some(changes) = changes {
                    self.rpc.notify("textDocument/didChange",json!({"textDocument":{"uri":uri,"version":version},"contentChanges":changes})).await?;
                } else if open_close {
                    // An open/close-only server has no legal didChange path.
                    self.rpc
                        .notify("textDocument/didClose", json!({"textDocument":{"uri":uri}}))
                        .await?;
                    self.rpc.notify("textDocument/didOpen",json!({"textDocument":{"uri":uri,"languageId":odin_core::languages::language_id_for_path(&self.language.id,std::path::Path::new(path)),"version":version,"text":content}})).await?;
                }
                document.version = version;
                document.hash = hash;
                document.content = content.clone();
            }
            _ => {}
        }
        Ok(content)
    }
    pub async fn refresh(&self, changed: &[String]) -> Result<()> {
        let opened = self
            .documents
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for path in opened {
            if self.root.resolve(&path, true)?.exists() {
                self.sync(&path).await?;
            } else {
                let abs = self.root.root().join(&path);
                let uri = Url::from_file_path(abs)
                    .map_err(|_| anyhow!("Invalid URI"))?
                    .to_string();
                self.rpc.invalidate_diagnostics(&uri)?;
                if self.sync_options().0 {
                    self.rpc
                        .notify("textDocument/didClose", json!({"textDocument":{"uri":uri}}))
                        .await?;
                }
                self.documents.lock().await.remove(&path);
            }
        }
        let mut events = Vec::new();
        for path in changed {
            if !self.matches(path) {
                continue;
            }
            let full = self.root.resolve(path, true)?;
            let uri = Url::from_file_path(&full)
                .map_err(|_| anyhow!("Invalid URI"))?
                .to_string();
            events.push(json!({"uri":uri,"type":if full.exists(){2}else{3}}));
        }
        if !events.is_empty() {
            self.rpc
                .notify("workspace/didChangeWatchedFiles", json!({"changes":events}))
                .await?;
        }
        Ok(())
    }
    fn require(&self, capability: &str) -> Result<()> {
        if !self.supports(capability) {
            bail!("CAPABILITY_UNAVAILABLE: {capability}");
        }
        Ok(())
    }
    async fn snapshot(&self, path: &str, expected: Option<&str>) -> Result<DocumentSnapshot> {
        let content = self.sync(path).await?;
        let hash = sha256(content.as_bytes());
        if expected.is_some_and(|expected| expected != hash) {
            bail!("CONFLICT: source hash changed before LSP request: {path}");
        }
        let documents = self.documents.lock().await;
        let document = documents
            .get(path)
            .ok_or_else(|| anyhow!("Missing synchronized document"))?;
        if document.hash != hash {
            bail!("CONFLICT: document changed while synchronizing: {path}");
        }
        Ok(DocumentSnapshot {
            path: path.into(),
            uri: self.uri(path)?,
            content,
            hash,
            version: document.version,
        })
    }
    async fn verify_snapshot(&self, snapshot: &DocumentSnapshot) -> Result<()> {
        if sha256(self.root.read(&snapshot.path)?.as_bytes()) != snapshot.hash {
            bail!(
                "CONFLICT: source changed while language server computed response: {}",
                snapshot.path
            );
        }
        let documents = self.documents.lock().await;
        if !documents.get(&snapshot.path).is_some_and(|document| {
            document.version == snapshot.version && document.hash == snapshot.hash
        }) {
            bail!(
                "CONFLICT: synchronized document version changed during LSP request: {}",
                snapshot.path
            );
        }
        Ok(())
    }
    fn envelope(&self, snapshot: &DocumentSnapshot, result: Value) -> Value {
        json!({"result":result,"provider":self.language.id,"source":"lsp","document":snapshot.metadata(),
            "position_encoding":"utf-16","freshness":"current","commands_executed":false,"edits_applied":false,"index_complete_proven":false})
    }
    async fn request_document(
        &self,
        snapshot: &DocumentSnapshot,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        let result = self.rpc.request(method, params).await?;
        self.verify_snapshot(snapshot).await?;
        Ok(result)
    }
    async fn request_at(
        &self,
        path: &str,
        position: Position,
        method: &str,
        capability: &str,
    ) -> Result<(DocumentSnapshot, Value)> {
        self.require(capability)?;
        let snapshot = self.snapshot(path, None).await?;
        crate::text::offset(&snapshot.content, position)?;
        let result = self
            .request_document(
                &snapshot,
                method,
                json!({"textDocument":{"uri":snapshot.uri},"position":position}),
            )
            .await?;
        Ok((snapshot, result))
    }
    pub async fn completion(&self, path: &str, position: Position) -> Result<Value> {
        let (snapshot, result) = self
            .request_at(
                path,
                position,
                "textDocument/completion",
                "completionProvider",
            )
            .await?;
        let items = if result.is_object() {
            if !result["isIncomplete"].is_boolean() {
                bail!("Invalid completion list completeness flag");
            }
            if !result["items"].is_array() {
                bail!("Invalid completion list items");
            }
            &result["items"]
        } else {
            &result
        };
        if !items.is_null() {
            for item in items
                .as_array()
                .ok_or_else(|| anyhow!("Invalid completion response"))?
            {
                validate_completion(item, &snapshot.content)?;
            }
        }
        Ok(self.envelope(&snapshot, result))
    }
    pub async fn resolve_completion(
        &self,
        path: &str,
        item: &Value,
        expected_sha256: &str,
    ) -> Result<Value> {
        if self.capabilities["completionProvider"]["resolveProvider"] != true {
            bail!("CAPABILITY_UNAVAILABLE: completionProvider.resolveProvider");
        }
        let snapshot = self.snapshot(path, Some(expected_sha256)).await?;
        validate_completion(item, &snapshot.content)?;
        let result = self
            .request_document(&snapshot, "completionItem/resolve", item.clone())
            .await?;
        validate_completion(&result, &snapshot.content)?;
        Ok(self.envelope(&snapshot, result))
    }
    pub async fn hover(&self, path: &str, position: Position) -> Result<Value> {
        let (snapshot, result) = self
            .request_at(path, position, "textDocument/hover", "hoverProvider")
            .await?;
        if !result.is_null() {
            if !result.is_object() || result.get("contents").is_none() {
                bail!("Invalid hover response");
            }
            if let Some(range) = result.get("range") {
                validate_range(&snapshot.content, range)?;
            }
        }
        Ok(self.envelope(&snapshot, result))
    }
    pub async fn signature_help(&self, path: &str, position: Position) -> Result<Value> {
        let (snapshot, result) = self
            .request_at(
                path,
                position,
                "textDocument/signatureHelp",
                "signatureHelpProvider",
            )
            .await?;
        if !result.is_null() {
            let signatures = result["signatures"]
                .as_array()
                .ok_or_else(|| anyhow!("Invalid signatureHelp response"))?;
            for signature in signatures {
                if !signature["label"].is_string() {
                    bail!("Invalid signature label");
                }
            }
            if let Some(active) = result.get("activeSignature") {
                let active = active
                    .as_u64()
                    .ok_or_else(|| anyhow!("Invalid active signature index"))?;
                if !signatures.is_empty() && active >= signatures.len() as u64 {
                    bail!("Signature index outside response");
                }
            }
        }
        Ok(self.envelope(&snapshot, result))
    }
    /// Returns an inert WorkspaceEdit. Only engine/core Transactions can apply it.
    pub async fn formatting(
        &self,
        path: &str,
        options: &Value,
        range: Option<Range>,
        expected_sha256: &str,
    ) -> Result<Value> {
        let (method, capability) = if range.is_some() {
            (
                "textDocument/rangeFormatting",
                "documentRangeFormattingProvider",
            )
        } else {
            ("textDocument/formatting", "documentFormattingProvider")
        };
        self.require(capability)?;
        if !options.is_object()
            || !options["tabSize"]
                .as_u64()
                .is_some_and(|size| (1..=32).contains(&size))
            || !options["insertSpaces"].is_boolean()
        {
            bail!("Formatting options require tabSize 1..32 and insertSpaces boolean");
        }
        for value in options
            .as_object()
            .ok_or_else(|| anyhow!("Invalid formatting options"))?
            .values()
        {
            if !value.is_boolean() && !value.is_string() && !value.is_number() {
                bail!("Invalid formatting option value");
            }
        }
        let snapshot = self.snapshot(path, Some(expected_sha256)).await?;
        let mut params = json!({"textDocument":{"uri":snapshot.uri},"options":options});
        if let Some(range) = range {
            validate_range(&snapshot.content, &serde_json::to_value(range)?)?;
            params["range"] = serde_json::to_value(range)?;
        }
        let result = self.request_document(&snapshot, method, params).await?;
        let edits = if result.is_null() { json!([]) } else { result };
        let parsed: Vec<TextEdit> = serde_json::from_value(edits.clone())?;
        crate::text::edit(&snapshot.content, &parsed)?;
        Ok(
            json!({"documentChanges":[{"textDocument":{"uri":snapshot.uri,"version":snapshot.version},"edits":edits}]}),
        )
    }
    pub async fn inlay_hints(&self, path: &str, range: Range) -> Result<Value> {
        self.require("inlayHintProvider")?;
        let snapshot = self.snapshot(path, None).await?;
        validate_range(&snapshot.content, &serde_json::to_value(range)?)?;
        let result = self
            .request_document(
                &snapshot,
                "textDocument/inlayHint",
                json!({"textDocument":{"uri":snapshot.uri},"range":range}),
            )
            .await?;
        if !result.is_null() {
            for hint in result
                .as_array()
                .ok_or_else(|| anyhow!("Invalid inlayHint response"))?
            {
                validate_hint(hint, &snapshot.content)?;
            }
        }
        Ok(self.envelope(&snapshot, result))
    }
    pub async fn resolve_inlay_hint(
        &self,
        path: &str,
        hint: &Value,
        expected_sha256: &str,
    ) -> Result<Value> {
        if self.capabilities["inlayHintProvider"]["resolveProvider"] != true {
            bail!("CAPABILITY_UNAVAILABLE: inlayHintProvider.resolveProvider");
        }
        let snapshot = self.snapshot(path, Some(expected_sha256)).await?;
        validate_hint(hint, &snapshot.content)?;
        let result = self
            .request_document(&snapshot, "inlayHint/resolve", hint.clone())
            .await?;
        validate_hint(&result, &snapshot.content)?;
        Ok(self.envelope(&snapshot, result))
    }
    pub async fn semantic_tokens(&self, path: &str, range: Option<Range>) -> Result<Value> {
        self.require("semanticTokensProvider")?;
        let provider = &self.capabilities["semanticTokensProvider"];
        let mode = if range.is_some() { "range" } else { "full" };
        if provider[mode] != true && !provider[mode].is_object() {
            bail!("CAPABILITY_UNAVAILABLE: semanticTokensProvider.{mode}");
        }
        let snapshot = self.snapshot(path, None).await?;
        let mut params = json!({"textDocument":{"uri":snapshot.uri}});
        if let Some(range) = range {
            validate_range(&snapshot.content, &serde_json::to_value(range)?)?;
            params["range"] = serde_json::to_value(range)?;
        }
        let result = self
            .request_document(
                &snapshot,
                &format!("textDocument/semanticTokens/{mode}"),
                params,
            )
            .await?;
        validate_semantic_tokens(&snapshot.content, &result, &provider["legend"])?;
        let mut output = self.envelope(&snapshot, result);
        output["legend"] = provider["legend"].clone();
        Ok(output)
    }
    fn workspace_hashes(&self, scope: Option<&str>) -> Result<BTreeMap<String, String>> {
        let full = self.root.resolve(scope.unwrap_or("."), false)?;
        let scope = if full == self.root.root() {
            ".".to_string()
        } else {
            self.root.relative(&full)?
        };
        let listing = odin_core::search::files(&self.root, 10000)?;
        if listing.truncated {
            bail!("LIMIT_EXCEEDED: workspace source snapshot exceeds 10000 files");
        }
        let mut hashes = BTreeMap::new();
        let mut bytes = 0usize;
        for path in listing.files {
            if !self.matches(&path)
                || !(scope == "." || scope == path || path.starts_with(&format!("{scope}/")))
            {
                continue;
            }
            let content = self.root.read(&path)?;
            bytes = bytes
                .checked_add(content.len())
                .ok_or_else(|| anyhow!("Source snapshot size overflow"))?;
            if bytes > 32 * 1024 * 1024 {
                bail!("LIMIT_EXCEEDED: workspace source snapshot exceeds 32 MiB");
            }
            hashes.insert(path, sha256(content.as_bytes()));
        }
        Ok(hashes)
    }
    fn verify_workspace(
        &self,
        scope: Option<&str>,
        before: &BTreeMap<String, String>,
    ) -> Result<()> {
        if &self.workspace_hashes(scope)? != before {
            bail!("CONFLICT: workspace source changed during LSP request");
        }
        Ok(())
    }
    pub async fn workspace_symbols(&self, query: &str) -> Result<Value> {
        self.workspace_symbols_scoped(query, None, 100).await
    }
    pub async fn workspace_symbols_scoped(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
    ) -> Result<Value> {
        self.require("workspaceSymbolProvider")?;
        if query.len() > 4096 || query.contains('\0') || !(1..=1000).contains(&limit) {
            bail!("Invalid workspace symbol query or limit (1..1000)");
        }
        let opened = self
            .documents
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for path in opened {
            if self.root.resolve(&path, true)?.exists() {
                self.sync(&path).await?;
            }
        }
        let before = self.workspace_hashes(scope)?;
        let result = self
            .rpc
            .request("workspace/symbol", json!({"query":query}))
            .await?;
        self.verify_workspace(scope, &before)?;
        let mut output = Vec::new();
        let mut eligible = 0usize;
        let mut excluded = 0usize;
        for symbol in nullable_array(&result, "workspace symbol")? {
            if !symbol["name"].is_string() || symbol["kind"].as_u64().is_none() {
                bail!("Invalid workspace symbol");
            }
            let location = &symbol["location"];
            let uri = location["uri"]
                .as_str()
                .ok_or_else(|| anyhow!("Workspace symbol has no URI"))?;
            let Ok(path) = self.relative_uri(uri) else {
                excluded += 1;
                continue;
            };
            let Some(hash) = before.get(&path) else {
                excluded += 1;
                continue;
            };
            let content = self.root.read(&path)?;
            if sha256(content.as_bytes()) != *hash {
                bail!("CONFLICT: workspace symbol source changed: {path}");
            }
            validate_range(&content, &location["range"])?;
            eligible += 1;
            if output.len() < limit {
                let mut symbol = symbol.clone();
                symbol["relative_path"] = json!(path);
                symbol["source_sha256"] = json!(hash);
                output.push(symbol);
            }
        }
        self.verify_workspace(scope, &before)?;
        Ok(
            json!({"result":output,"provider":self.language.id,"source":"lsp","position_encoding":"utf-16",
            "freshness":"current","source_scope":"project_local_matching_language_files","scope":scope.unwrap_or("."),
            "limit":limit,"total_in_scope":eligible,"excluded_outside_snapshot":excluded,"truncated":eligible>limit,
            "index_complete_proven":false,"commands_executed":false}),
        )
    }
    pub async fn call_hierarchy(
        &self,
        path: &str,
        position: Position,
        direction: &str,
    ) -> Result<Value> {
        let method = match direction {
            "incoming" => "callHierarchy/incomingCalls",
            "outgoing" => "callHierarchy/outgoingCalls",
            _ => bail!("direction must be incoming or outgoing"),
        };
        self.hierarchy_response(
            path,
            position,
            "textDocument/prepareCallHierarchy",
            method,
            "callHierarchyProvider",
            true,
        )
        .await
    }
    async fn hierarchy_response(
        &self,
        path: &str,
        position: Position,
        prepare: &str,
        method: &str,
        capability: &str,
        calls: bool,
    ) -> Result<Value> {
        self.require(capability)?;
        let deadline = Instant::now()
            .checked_add(self.timeout)
            .ok_or_else(|| anyhow!("Invalid hierarchy query timeout"))?;
        tokio::time::timeout_at(deadline, async {
            let mut budget = HierarchyBudget::new(deadline);
            budget.check_deadline()?;
            let before = self.workspace_hashes(None)?;
            budget.check_deadline()?;
            let snapshot = self.snapshot(path, None).await?;
            crate::text::offset(&snapshot.content, position)?;
            budget.check_deadline()?;
            let items = self
                .request_document(
                    &snapshot,
                    prepare,
                    json!({"textDocument":{"uri":snapshot.uri},"position":position}),
                )
                .await?;
            let prepared = match items {
                Value::Null => Vec::new(),
                Value::Array(items) => items,
                _ => bail!("Invalid hierarchy prepare response; expected array or null"),
            };
            if prepared.len() > HIERARCHY_PREPARE_LIMIT {
                bail!("LIMIT_EXCEEDED: hierarchy prepare exceeds {HIERARCHY_PREPARE_LIMIT} items");
            }
            // Reserve every prepared item before the first related request. Otherwise
            // unprocessed items and accumulated output could each consume the quota.
            for (index, item) in prepared.iter().enumerate() {
                budget.reserve_item(item, index > 0)?;
            }
            let mut output = Vec::with_capacity(prepared.len());
            for item in prepared {
                budget.check_deadline()?;
                self.validate_hierarchy_item(&item, &before)?;
                // Check before json! clones the item into the outgoing request.
                budget.check_deadline()?;
                let related = self
                    .request_document(&snapshot, method, json!({"item":item}))
                    .await?;
                let relations = nullable_array(&related, "hierarchy")?;
                budget.add_relations(relations.len())?;
                // Measure before validation or retaining/cloning the RPC response.
                budget.accept_related(&related)?;
                for relation in relations {
                    budget.check_deadline()?;
                    if calls {
                        let incoming = method.ends_with("incomingCalls");
                        let target = if incoming {
                            &relation["from"]
                        } else {
                            &relation["to"]
                        };
                        self.validate_hierarchy_item(target, &before)?;
                        budget.check_deadline()?;
                        let source = if incoming { target } else { &item };
                        let source_path = self.relative_uri(
                            source["uri"]
                                .as_str()
                                .ok_or_else(|| anyhow!("Missing hierarchy URI"))?,
                        )?;
                        let ranges = relation["fromRanges"]
                            .as_array()
                            .ok_or_else(|| anyhow!("Invalid call hierarchy ranges"))?;
                        budget.add_ranges(ranges.len())?;
                        let text = self.root.read(&source_path)?;
                        for range in ranges {
                            budget.check_deadline()?;
                            validate_range(&text, range)?;
                        }
                    } else {
                        self.validate_hierarchy_item(relation, &before)?;
                    }
                }
                budget.check_deadline()?;
                output.push(Value::Object(serde_json::Map::from_iter([
                    ("item".into(), item),
                    ("related".into(), related),
                ])));
            }
            budget.check_deadline()?;
            self.verify_workspace(None, &before)?;
            budget.check_deadline()?;
            self.verify_snapshot(&snapshot).await?;
            budget.check_deadline()?;
            // Move the result into the envelope, then make only the bounded alias
            // required by the existing API (at most 8 MiB across both fields).
            let mut response = self.envelope(&snapshot, Value::Null);
            response["result"] = Value::Array(output);
            response["hierarchy"] = response["result"].clone();
            budget.check_deadline()?;
            Ok(response)
        })
        .await
        .map_err(|_| anyhow!(HIERARCHY_TIMEOUT))?
    }
    fn validate_hierarchy_item(
        &self,
        item: &Value,
        before: &BTreeMap<String, String>,
    ) -> Result<()> {
        if !item["name"].is_string() || item["kind"].as_u64().is_none() {
            bail!("Invalid hierarchy item");
        }
        let path = self.relative_uri(
            item["uri"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing hierarchy URI"))?,
        )?;
        let content = self.root.read(&path)?;
        if before.get(&path) != Some(&sha256(content.as_bytes())) {
            bail!("CONFLICT: hierarchy source outside verified snapshot or changed: {path}");
        }
        validate_range(&content, &item["range"])?;
        validate_range(&content, &item["selectionRange"])?;
        Ok(())
    }
    pub async fn symbols(&self, path: &str) -> Result<Vec<Symbol>> {
        let snapshot = self.snapshot(path, None).await?;
        if !self.supports("documentSymbolProvider") {
            bail!("CAPABILITY_UNAVAILABLE: documentSymbolProvider");
        }
        let result = self
            .request_document(
                &snapshot,
                "textDocument/documentSymbol",
                json!({"textDocument":{"uri":self.uri(path)?}}),
            )
            .await?;
        let mut output = Vec::new();
        if result.is_null() {
            return Ok(output);
        }
        for item in result
            .as_array()
            .ok_or_else(|| anyhow!("Invalid documentSymbol response"))?
        {
            if let Some(uri) = item["location"]["uri"].as_str()
                && self.relative_uri(uri)? != path
            {
                bail!("Document symbol points outside the requested file");
            }
            flatten_symbol(item, path, "", &mut output)?;
        }
        for symbol in &output {
            validate_range(&snapshot.content, &serde_json::to_value(symbol.range)?)?;
            validate_range(
                &snapshot.content,
                &serde_json::to_value(symbol.selection_range)?,
            )?;
        }
        Ok(output)
    }
    pub async fn symbol(&self, path: &str, name_path: &str) -> Result<Symbol> {
        let matches = self
            .symbols(path)
            .await?
            .into_iter()
            .filter(|s| s.name_path == name_path)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            bail!(
                "Expected exactly one symbol named {name_path}; found {}",
                matches.len()
            );
        }
        matches
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Symbol missing"))
    }
    pub async fn at(&self, path: &str, position: Position, kind: &str) -> Result<Value> {
        let (method, capability) = match kind {
            "references" => ("textDocument/references", "referencesProvider"),
            "definition" => ("textDocument/definition", "definitionProvider"),
            "declaration" => ("textDocument/declaration", "declarationProvider"),
            "implementation" => ("textDocument/implementation", "implementationProvider"),
            "hover" => ("textDocument/hover", "hoverProvider"),
            _ => bail!("Unknown semantic query"),
        };
        self.require(capability)?;
        let snapshot = self.snapshot(path, None).await?;
        crate::text::offset(&snapshot.content, position)?;
        let mut params = json!({"textDocument":{"uri":snapshot.uri},"position":position});
        if kind == "references" {
            params["context"] = json!({"includeDeclaration":false});
        }
        let result = self.request_document(&snapshot, method, params).await?;
        if kind != "hover" {
            validate_locations(&result)?;
        }
        Ok(result)
    }
    pub async fn diagnostics(&self, path: &str) -> Result<Value> {
        let snapshot = self.snapshot(path, None).await?;
        let (report, source, freshness) = if self.supports("diagnosticProvider") {
            let report = self
                .request_document(
                    &snapshot,
                    "textDocument/diagnostic",
                    json!({"textDocument":{"uri":snapshot.uri}}),
                )
                .await?;
            if report["kind"] != "full" || !report["items"].is_array() {
                bail!("Invalid full diagnostic report without previousResultId");
            }
            (Some(report), "lsp_pull", "current")
        } else {
            let report = self.rpc.diagnostics(&snapshot.uri)?;
            let freshness = match &report {
                None => "pending",
                Some(report) => match report.get("version") {
                    None | Some(Value::Null) => "unverified",
                    Some(version) => {
                        if version
                            .as_i64()
                            .ok_or_else(|| anyhow!("Invalid diagnostic document version"))?
                            == snapshot.version
                        {
                            "current"
                        } else {
                            "stale"
                        }
                    }
                },
            };
            (report, "lsp_push", freshness)
        };
        self.verify_snapshot(&snapshot).await?;
        let response = diagnostic_output(
            &snapshot,
            report.as_ref(),
            source,
            &self.language.id,
            freshness,
        )?;
        self.verify_snapshot(&snapshot).await?;
        Ok(response)
    }
    pub async fn rename(
        &self,
        path: &str,
        position: Position,
        new_name: &str,
        expected_sha256: &str,
    ) -> Result<Value> {
        self.require("renameProvider")?;
        if new_name.is_empty() || new_name.contains(['\n', '\r', '\0']) {
            bail!("Invalid new symbol name");
        }
        let snapshot = self.snapshot(path, Some(expected_sha256)).await?;
        crate::text::offset(&snapshot.content, position)?;
        let params = json!({"textDocument":{"uri":snapshot.uri},"position":position});
        if self.capabilities["renameProvider"]["prepareProvider"] == true {
            let prepared = self
                .request_document(&snapshot, "textDocument/prepareRename", params.clone())
                .await?;
            if prepared.is_null() {
                bail!("Language server rejected rename at this position");
            }
        }
        let mut params = params;
        params["newName"] = json!(new_name);
        self.request_document(&snapshot, "textDocument/rename", params)
            .await
    }
    pub async fn hierarchy(
        &self,
        path: &str,
        position: Position,
        direction: &str,
    ) -> Result<Value> {
        let method = match direction {
            "super" => "typeHierarchy/supertypes",
            "sub" => "typeHierarchy/subtypes",
            _ => bail!("direction must be super or sub"),
        };
        self.hierarchy_response(
            path,
            position,
            "textDocument/prepareTypeHierarchy",
            method,
            "typeHierarchyProvider",
            false,
        )
        .await
    }
    pub async fn actions(&self, path: &str, range: Range) -> Result<Value> {
        self.actions_with_diagnostics(path, range, &[]).await
    }
    pub async fn actions_with_diagnostics(
        &self,
        path: &str,
        range: Range,
        diagnostics: &[Value],
    ) -> Result<Value> {
        self.require("codeActionProvider")?;
        let snapshot = self.snapshot(path, None).await?;
        validate_range(&snapshot.content, &serde_json::to_value(range)?)?;
        self.request_document(&snapshot,"textDocument/codeAction",json!({"textDocument":{"uri":snapshot.uri},"range":range,"context":{"diagnostics":diagnostics}})).await
    }
    pub async fn resolve_action(&self, action: &Value) -> Result<Value> {
        if self.capabilities["codeActionProvider"]["resolveProvider"] != true {
            bail!("CAPABILITY_UNAVAILABLE: codeActionProvider.resolveProvider");
        }
        if !action.is_object() {
            bail!("Code action must be an object");
        }
        self.rpc.request("codeAction/resolve", action.clone()).await
    }
    /// Replay text and resource operations in memory. The caller must pass the
    /// resulting changes to core Transactions; this never writes project files.
    pub async fn changes(
        &self,
        workspace_edit: &Value,
        snapshots: &BTreeMap<String, String>,
    ) -> Result<Vec<Change>> {
        let versions = self
            .documents
            .lock()
            .await
            .iter()
            .map(|(path, doc)| (path.clone(), doc.version))
            .collect();
        Planner::new(&self.root, snapshots, &versions).plan(workspace_edit)
    }
}
struct HierarchyBudget {
    deadline: Instant,
    retained_bytes: usize,
    relations: usize,
    ranges: usize,
}
#[derive(Serialize)]
struct HierarchyEntry<'a> {
    item: &'a Value,
    related: &'a Value,
}
impl HierarchyBudget {
    fn new(deadline: Instant) -> Self {
        Self {
            deadline,
            retained_bytes: 2, // The result array's brackets.
            relations: 0,
            ranges: 0,
        }
    }
    fn check_deadline(&self) -> Result<()> {
        if Instant::now() >= self.deadline {
            bail!(HIERARCHY_TIMEOUT);
        }
        Ok(())
    }
    fn reserve_item(&mut self, item: &Value, separator: bool) -> Result<()> {
        self.check_deadline()?;
        let remaining = HIERARCHY_JSON_LIMIT
            .checked_sub(self.retained_bytes + usize::from(separator))
            .ok_or_else(hierarchy_json_limit)?;
        let size = hierarchy_json_size(
            &HierarchyEntry {
                item,
                related: &Value::Null,
            },
            remaining,
            self.deadline,
        )?;
        self.retained_bytes += size + usize::from(separator);
        Ok(())
    }
    fn accept_related(&mut self, related: &Value) -> Result<()> {
        self.check_deadline()?;
        // Replace this item's four-byte reserved `null` without allocating JSON.
        let base = self.retained_bytes - 4;
        let size = hierarchy_json_size(related, HIERARCHY_JSON_LIMIT - base, self.deadline)?;
        self.retained_bytes = base + size;
        Ok(())
    }
    fn add_relations(&mut self, count: usize) -> Result<()> {
        if count > HIERARCHY_RELATION_LIMIT - self.relations {
            bail!("LIMIT_EXCEEDED: hierarchy exceeds {HIERARCHY_RELATION_LIMIT} related items");
        }
        self.relations += count;
        Ok(())
    }
    fn add_ranges(&mut self, count: usize) -> Result<()> {
        if count > HIERARCHY_RANGE_LIMIT - self.ranges {
            bail!("LIMIT_EXCEEDED: hierarchy exceeds {HIERARCHY_RANGE_LIMIT} call ranges");
        }
        self.ranges += count;
        Ok(())
    }
}
fn hierarchy_json_limit() -> anyhow::Error {
    anyhow!("LIMIT_EXCEEDED: hierarchy retained JSON exceeds {HIERARCHY_JSON_LIMIT} bytes")
}
fn hierarchy_json_size(value: &impl Serialize, limit: usize, deadline: Instant) -> Result<usize> {
    struct Counter {
        bytes: usize,
        limit: usize,
        deadline: Instant,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if Instant::now() >= self.deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    HIERARCHY_TIMEOUT,
                ));
            }
            if bytes.len() > self.limit - self.bytes {
                return Err(std::io::Error::other(hierarchy_json_limit()));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter {
        bytes: 0,
        limit,
        deadline,
    };
    serde_json::to_writer(&mut counter, value)?;
    Ok(counter.bytes)
}
fn nullable_array<'a>(value: &'a Value, kind: &str) -> Result<&'a [Value]> {
    if value.is_null() {
        return Ok(&[]);
    }
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| anyhow!("Invalid {kind} response; expected array or null"))
}
fn validate_range(content: &str, value: &Value) -> Result<Range> {
    let range: Range = serde_json::from_value(value.clone())?;
    if crate::text::offset(content, range.start)? > crate::text::offset(content, range.end)? {
        bail!("Reversed LSP range");
    }
    Ok(range)
}
fn validate_locations(value: &Value) -> Result<()> {
    if value.is_null() {
        return Ok(());
    }
    if let Some(items) = value.as_array() {
        for item in items {
            validate_locations(item)?;
        }
        return Ok(());
    }
    let (uri, range) = if value.get("targetUri").is_some() {
        (&value["targetUri"], &value["targetRange"])
    } else {
        (&value["uri"], &value["range"])
    };
    let uri = uri
        .as_str()
        .ok_or_else(|| anyhow!("Invalid location URI"))?;
    Url::parse(uri)?;
    let range: Range = serde_json::from_value(range.clone())?;
    if range.start > range.end {
        bail!("Reversed location range");
    }
    Ok(())
}
fn validate_completion(item: &Value, content: &str) -> Result<()> {
    if !item.is_object() || !item["label"].is_string() {
        bail!("Invalid completion item");
    }
    if item["insertTextFormat"] == 2 {
        bail!("CAPABILITY_UNAVAILABLE: completion snippets were not negotiated");
    }
    if let Some(edit) = item.get("textEdit") {
        if !edit["newText"].is_string() {
            bail!("Invalid completion edit text");
        }
        if let Some(range) = edit.get("range") {
            validate_range(content, range)?;
        } else {
            let insert = validate_range(content, &edit["insert"])?;
            let replace = validate_range(content, &edit["replace"])?;
            if insert.start != replace.start || insert.end > replace.end {
                bail!("Invalid insert/replace completion edit");
            }
        }
    }
    if let Some(edits) = item.get("additionalTextEdits") {
        let parsed: Vec<TextEdit> = serde_json::from_value(edits.clone())?;
        crate::text::edit(content, &parsed)?;
    }
    Ok(())
}
fn validate_hint(hint: &Value, content: &str) -> Result<()> {
    let position: Position = serde_json::from_value(hint["position"].clone())?;
    crate::text::offset(content, position)?;
    if let Some(label) = hint["label"].as_str() {
        if label.is_empty() {
            bail!("Inlay hint label is empty");
        }
    } else {
        let parts = hint["label"]
            .as_array()
            .ok_or_else(|| anyhow!("Invalid inlay hint label"))?;
        if parts.is_empty()
            || parts
                .iter()
                .any(|part| part["value"].as_str().is_none_or(|value| value.is_empty()))
        {
            bail!("Invalid inlay hint label parts");
        }
    }
    if let Some(edits) = hint.get("textEdits") {
        let parsed: Vec<TextEdit> = serde_json::from_value(edits.clone())?;
        crate::text::edit(content, &parsed)?;
    }
    Ok(())
}
fn validate_semantic_tokens(content: &str, result: &Value, legend: &Value) -> Result<()> {
    let types = legend["tokenTypes"]
        .as_array()
        .ok_or_else(|| anyhow!("Missing semantic token type legend"))?;
    let modifiers = legend["tokenModifiers"]
        .as_array()
        .ok_or_else(|| anyhow!("Missing semantic token modifier legend"))?;
    if types.len() > 65536
        || modifiers.len() > 31
        || types.iter().chain(modifiers).any(|item| !item.is_string())
    {
        bail!("Invalid semantic token legend");
    }
    if result.is_null() {
        return Ok(());
    }
    let data = result["data"]
        .as_array()
        .ok_or_else(|| anyhow!("Invalid semantic token data"))?;
    if data.len() % 5 != 0 {
        bail!("Semantic token data must contain groups of five integers");
    }
    let mut line = 0u32;
    let mut character = 0u32;
    let mut previous_end = Position {
        line: 0,
        character: 0,
    };
    let mut boundaries = std::collections::BTreeSet::new();
    for encoded in data.chunks_exact(5) {
        let mut token = [0u32; 5];
        for (out, value) in token.iter_mut().zip(encoded) {
            *out = value
                .as_u64()
                .filter(|n| *n <= i32::MAX as u64)
                .ok_or_else(|| anyhow!("Invalid semantic token integer"))?
                as u32;
        }
        line = line
            .checked_add(token[0])
            .ok_or_else(|| anyhow!("Semantic token line overflow"))?;
        character = if token[0] == 0 {
            character
                .checked_add(token[1])
                .ok_or_else(|| anyhow!("Semantic token column overflow"))?
        } else {
            token[1]
        };
        let start = Position { line, character };
        let end = Position {
            line,
            character: character
                .checked_add(token[2])
                .ok_or_else(|| anyhow!("Semantic token length overflow"))?,
        };
        if start < previous_end
            || token[3] as usize >= types.len()
            || (modifiers.len() < 31 && token[4] >> modifiers.len() != 0)
        {
            bail!(
                "Semantic token is outside the document, overlaps, splits UTF-16, or references an invalid legend index"
            );
        }
        boundaries.insert(start);
        boundaries.insert(end);
        previous_end = end;
    }
    // Retain only requested token boundaries, never a node per source character.
    // One document pass validates UTF-16 boundaries and bare-CR/CRLF lines.
    let mut requested = boundaries.into_iter().peekable();
    let mut position = Position {
        line: 0,
        character: 0,
    };
    let mut chars = content.chars().peekable();
    loop {
        while requested.peek() == Some(&position) {
            requested.next();
        }
        if requested.peek().is_some_and(|next| *next < position) {
            bail!("Semantic token splits UTF-16 or extends past a line");
        }
        if requested.peek().is_none() {
            break;
        }
        let Some(ch) = chars.next() else {
            bail!("Semantic token is outside the document");
        };
        if ch == '\r' || ch == '\n' {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            position.line += 1;
            position.character = 0;
        } else {
            position.character += ch.len_utf16() as u32;
        }
    }
    Ok(())
}
fn diagnostic_output(
    snapshot: &DocumentSnapshot,
    report: Option<&Value>,
    source: &str,
    provider: &str,
    freshness: &str,
) -> Result<Value> {
    let current = freshness == "current";
    let available = report.is_some() && freshness != "stale";
    let mut findings = Vec::new();
    let mut counts = [0usize; 5];
    if let Some(report) = report {
        let diagnostics = if source == "lsp_pull" {
            &report["items"]
        } else {
            &report["diagnostics"]
        };
        for diagnostic in diagnostics
            .as_array()
            .ok_or_else(|| anyhow!("Invalid diagnostic report items"))?
        {
            let range = if current {
                validate_range(&snapshot.content, &diagnostic["range"])?
            } else {
                let range: Range = serde_json::from_value(diagnostic["range"].clone())?;
                if range.start > range.end {
                    bail!("Reversed diagnostic range");
                }
                range
            };
            let message = diagnostic["message"]
                .as_str()
                .ok_or_else(|| anyhow!("Diagnostic message is missing"))?;
            let (severity, index) = match diagnostic.get("severity") {
                Some(value) if value.as_u64() == Some(1) => ("error", 0),
                Some(value) if value.as_u64() == Some(2) => ("warning", 1),
                Some(value) if value.as_u64() == Some(3) => ("information", 2),
                Some(value) if value.as_u64() == Some(4) => ("hint", 3),
                None | Some(Value::Null) => ("unknown", 4),
                _ => bail!("Invalid diagnostic severity"),
            };
            counts[index] += 1;
            findings.push(json!({"provider":provider,"source":diagnostic["source"].as_str().unwrap_or(source),
                "rule_id":diagnostic.get("code").cloned().unwrap_or(Value::Null),"relative_path":snapshot.path,"range":range,
                "severity":severity,"lsp_severity":diagnostic["severity"],"document_version":if source=="lsp_pull"{json!(snapshot.version)}else{report["version"].clone()},
                "status":freshness,"message":message,"source_sha256":if current{Some(&snapshot.hash)}else{None}}));
        }
    }
    let status = if !available {
        freshness
    } else if !current {
        "unverified"
    } else if counts[0] > 0 {
        "errors"
    } else if counts[1] > 0 {
        "warnings"
    } else if findings.is_empty() {
        "clean"
    } else {
        "findings"
    };
    Ok(
        json!({"available":available,"pending":!available,"stale":freshness=="stale","source":source,"provider":provider,
        "report":report,"document":snapshot.metadata(),"current_version":snapshot.version,"reported_version":report.and_then(|r|r.get("version")),
        "version_verified":current,"freshness":freshness,"status":status,"findings":findings,
        "summary":{"errors":counts[0],"warnings":counts[1],"information":counts[2],"hints":counts[3],"unknown":counts[4],
            "total":findings.len(),"complete":current&&available,"scope":"document"},
        "clean_build_proven":false,"position_encoding":"utf-16",
        "reason":if !available{Some("No current diagnostics received; this does not prove zero errors")}else{None}}),
    )
}
fn flatten_symbol(value: &Value, path: &str, parent: &str, output: &mut Vec<Symbol>) -> Result<()> {
    let name = value["name"]
        .as_str()
        .ok_or_else(|| anyhow!("Symbol has no name"))?
        .to_owned();
    let range: Range = serde_json::from_value(
        value
            .get("range")
            .or_else(|| value.get("location").and_then(|v| v.get("range")))
            .cloned()
            .ok_or_else(|| anyhow!("Symbol has no range"))?,
    )?;
    let selection_range = serde_json::from_value(
        value
            .get("selectionRange")
            .cloned()
            .unwrap_or(serde_json::to_value(range)?),
    )?;
    let parent = if parent.is_empty() {
        value
            .get("containerName")
            .and_then(Value::as_str)
            .unwrap_or("")
    } else {
        parent
    };
    let name_path = if parent.is_empty() {
        name.clone()
    } else {
        format!("{parent}/{name}")
    };
    output.push(Symbol {
        name,
        name_path: name_path.clone(),
        kind: value["kind"]
            .as_u64()
            .ok_or_else(|| anyhow!("Symbol kind is missing or invalid"))?,
        relative_path: path.into(),
        range,
        selection_range,
    });
    if let Some(children) = value["children"].as_array() {
        for child in children {
            flatten_symbol(child, path, &name_path, output)?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hierarchy_budget_counts_escaped_utf8_and_all_retained_fields() {
        let item = json!({"data":"\"\\\n😀"});
        let related = json!([{"data":"\t世界"}]);
        let mut budget = HierarchyBudget::new(Instant::now() + Duration::from_secs(1));
        budget.reserve_item(&item, false).unwrap();
        budget.reserve_item(&item, true).unwrap();
        budget.accept_related(&related).unwrap();
        budget.accept_related(&Value::Null).unwrap();
        let result = json!([
            {"item":item,"related":related},
            {"item":item,"related":null},
        ]);
        assert_eq!(
            budget.retained_bytes,
            serde_json::to_vec(&result).unwrap().len()
        );
        assert!(budget.retained_bytes * 2 <= HIERARCHY_JSON_LIMIT * 2);
        // Exercise the real cumulative byte limit without making a 4 MiB
        // serialization finish before a wall-clock deadline on a slow target.
        // Deadline rejection has its own deterministic expired-budget test.
        let mut nearly_full = HierarchyBudget::new(Instant::now() + Duration::from_secs(1));
        nearly_full.retained_bytes = HIERARCHY_JSON_LIMIT - 8;
        let before = nearly_full.retained_bytes;
        let error = nearly_full.reserve_item(&item, true).unwrap_err();
        assert!(error.to_string().contains("LIMIT_EXCEEDED"), "{error:#}");
        assert_eq!(nearly_full.retained_bytes, before);
        budget.add_relations(HIERARCHY_RELATION_LIMIT / 2).unwrap();
        budget.add_relations(HIERARCHY_RELATION_LIMIT / 2).unwrap();
        assert!(budget.add_relations(1).is_err());
        budget.add_ranges(HIERARCHY_RANGE_LIMIT).unwrap();
        assert!(budget.add_ranges(1).is_err());
    }
    #[test]
    fn hierarchy_budget_rejects_expired_synchronous_serialization() {
        let expired = Instant::now() - Duration::from_millis(1);
        let error = hierarchy_json_size(&json!({"data":"x"}), 100, expired).unwrap_err();
        assert!(error.to_string().contains("TIMEOUT"));
        assert!(HierarchyBudget::new(expired).check_deadline().is_err());
    }
    #[test]
    fn semantic_tokens_reject_surrogate_splits_overlaps_and_bad_legends() {
        let legend = json!({"tokenTypes":["function"],"tokenModifiers":["declaration"]});
        assert!(
            validate_semantic_tokens("fn 😀() {}\r\n", &json!({"data":[0,3,2,0,1]}), &legend)
                .is_ok()
        );
        for data in [
            json!([0, 4, 1, 0, 0]),
            json!([0, 3, 2, 0, 0, 0, 1, 1, 0, 0]),
            json!([0, 3, 2, 1, 0]),
            json!([0, 3, 2, 0, 2]),
            json!([0, 3]),
            json!([0, -1, 1, 0, 0]),
            json!([99, 0, 1, 0, 0]),
        ] {
            assert!(
                validate_semantic_tokens("fn 😀() {}\r\n", &json!({"data":data}), &legend).is_err()
            );
        }
    }
    #[test]
    fn diagnostic_findings_have_provenance_and_pending_never_means_clean() {
        let snapshot = DocumentSnapshot {
            path: "lib.rs".into(),
            uri: "file:///lib.rs".into(),
            content: "fn foo() {}".into(),
            hash: sha256(b"fn foo() {}"),
            version: 7,
        };
        let report = json!({"kind":"full","items":[{"range":{"start":{"line":0,"character":3},"end":{"line":0,"character":6}},"message":"broken","severity":1,"code":"E1","source":"fixture"}]});
        let output =
            diagnostic_output(&snapshot, Some(&report), "lsp_pull", "rust", "current").unwrap();
        assert_eq!(output["summary"]["errors"], 1);
        assert_eq!(output["summary"]["complete"], true);
        assert_eq!(output["status"], "errors");
        assert_eq!(output["findings"][0]["rule_id"], "E1");
        assert_eq!(output["findings"][0]["document_version"], 7);
        assert_eq!(output["findings"][0]["source_sha256"], snapshot.hash);
        let pending = diagnostic_output(&snapshot, None, "lsp_push", "rust", "pending").unwrap();
        assert_eq!(pending["status"], "pending");
        assert_eq!(pending["summary"]["complete"], false);
        let unverified = diagnostic_output(
            &snapshot,
            Some(&json!({"diagnostics":[]})),
            "lsp_push",
            "rust",
            "unverified",
        )
        .unwrap();
        assert_eq!(unverified["status"], "unverified");
        assert_eq!(unverified["summary"]["complete"], false);
        let stale=diagnostic_output(&snapshot,Some(&json!({"version":1,"diagnostics":[{"range":{"start":{"line":50,"character":0},"end":{"line":50,"character":9}},"message":"old error","severity":1}]})),"lsp_push","rust","stale").unwrap();
        assert_eq!(stale["status"], "stale");
        assert_eq!(stale["available"], false);
        assert!(stale["findings"][0]["source_sha256"].is_null());
        assert!(
            diagnostic_output(
                &snapshot,
                Some(&json!({"items":"invalid"})),
                "lsp_pull",
                "rust",
                "current"
            )
            .is_err()
        );
    }
    #[test]
    fn nested_symbols_keep_name_paths() {
        let range = json!({"start":{"line":0,"character":0},"end":{"line":3,"character":0}});
        let item = json!({"name":"A","kind":5,"range":range,"children":[{"name":"run","kind":6,"range":range}]});
        let mut out = Vec::new();
        flatten_symbol(&item, "a.rs", "", &mut out).unwrap();
        assert_eq!(out[1].name_path, "A/run");
    }
}
