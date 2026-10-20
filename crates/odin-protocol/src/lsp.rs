use crate::{
    rpc::{Dialect, Framing, RpcProcess},
    text::{Position, Range},
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
use tokio::sync::Mutex;
use url::Url;

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
pub struct LspClient {
    rpc: Arc<RpcProcess>,
    pub capabilities: Value,
    pub language: LanguageConfig,
    root: SafeRoot,
    documents: Mutex<BTreeMap<String, OpenDocument>>,
    shutting_down: AtomicBool,
    next_version: AtomicI64,
}
impl LspClient {
    pub async fn start(
        root: SafeRoot,
        language: LanguageConfig,
        timeout: Duration,
    ) -> Result<Arc<Self>> {
        let uri = Url::from_directory_path(root.root())
            .map_err(|_| anyhow!("Invalid workspace path"))?
            .to_string();
        let workspace = json!([{"uri":uri,"name":"odin-workspace"}]);
        let rpc = RpcProcess::spawn(
            &language.program,
            &language.args,
            root.root(),
            Framing::ContentLength,
            Dialect::JsonRpc,
            timeout,
            workspace.clone(),
            language.settings.clone(),
        )?;
        let result=rpc.request("initialize",json!({
            "processId":std::process::id(),"clientInfo":{"name":"odin","version":env!("CARGO_PKG_VERSION")},
            "rootUri":uri,"workspaceFolders":workspace,"initializationOptions":language.initialization_options,
            "capabilities":{
                "general":{"positionEncodings":["utf-16"]},
                "workspace":{"configuration":true,"workspaceFolders":true,"applyEdit":false,
                    "workspaceEdit":{"documentChanges":true,"resourceOperations":["create","rename","delete"]}},
                "textDocument":{"documentSymbol":{"hierarchicalDocumentSymbolSupport":true},
                    "synchronization":{"dynamicRegistration":false,"didSave":false},
                    "codeAction":{"dataSupport":true,"resolveSupport":{"properties":["edit"]}},
                    "publishDiagnostics":{"versionSupport":true},"diagnostic":{"dynamicRegistration":false}}
            }
        })).await?;
        let capabilities = result["capabilities"].clone();
        if capabilities
            .get("positionEncoding")
            .and_then(Value::as_str)
            .is_some_and(|s| s != "utf-16")
        {
            bail!("Language server selected a position encoding other than UTF-16");
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
            .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
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
                    self.rpc.notify("textDocument/didOpen",json!({"textDocument":{"uri":uri,"languageId":self.language.id,"version":version,"text":content}})).await?;
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
                    self.rpc.notify("textDocument/didOpen",json!({"textDocument":{"uri":uri,"languageId":self.language.id,"version":version,"text":content}})).await?;
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
    pub async fn symbols(&self, path: &str) -> Result<Vec<Symbol>> {
        self.sync(path).await?;
        if !self.supports("documentSymbolProvider") {
            bail!("CAPABILITY_UNAVAILABLE: documentSymbolProvider");
        }
        let result = self
            .rpc
            .request(
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
            flatten_symbol(item, path, "", &mut output)?;
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
        let content = self.sync(path).await?;
        crate::text::offset(&content, position)?;
        let (method, capability) = match kind {
            "references" => ("textDocument/references", "referencesProvider"),
            "definition" => ("textDocument/definition", "definitionProvider"),
            "declaration" => ("textDocument/declaration", "declarationProvider"),
            "implementation" => ("textDocument/implementation", "implementationProvider"),
            "hover" => ("textDocument/hover", "hoverProvider"),
            _ => bail!("Unknown semantic query"),
        };
        if !self.supports(capability) {
            bail!("CAPABILITY_UNAVAILABLE: {capability}");
        }
        let mut params = json!({"textDocument":{"uri":self.uri(path)?},"position":position});
        if kind == "references" {
            params["context"] = json!({"includeDeclaration":false});
        }
        self.rpc.request(method, params).await
    }
    pub async fn diagnostics(&self, path: &str) -> Result<Value> {
        let content = self.sync(path).await?;
        let uri = self.uri(path)?;
        if self.supports("diagnosticProvider") {
            let version = self.documents.lock().await.get(path).map(|d| d.version);
            let report = self
                .rpc
                .request(
                    "textDocument/diagnostic",
                    json!({"textDocument":{"uri":uri}}),
                )
                .await?;
            if report["kind"] != "full" || !report["items"].is_array() {
                bail!("Invalid full diagnostic report without previousResultId");
            }
            if self.root.read(path)? != content
                || self.documents.lock().await.get(path).map(|d| d.version) != version
            {
                bail!("File changed while language server computed diagnostics");
            }
            return Ok(
                json!({"available":true,"source":"lsp_pull","report":report,"current_version":version,"clean_build_proven":false}),
            );
        }
        let report = self.rpc.diagnostics(&uri)?;
        if self.root.read(path)? != content {
            bail!("File changed while reading diagnostics");
        }
        let current_version = self.documents.lock().await.get(path).map(|d| d.version);
        match report {
            Some(report) => {
                let reported = report.get("version").and_then(Value::as_i64);
                let verified = reported.is_some() && reported == current_version;
                if reported.is_some() && !verified {
                    return Ok(
                        json!({"available":false,"pending":true,"stale":true,"reported_version":reported,"current_version":current_version}),
                    );
                }
                Ok(
                    json!({"available":true,"source":"lsp_push","version_verified":verified,"current_version":current_version,"report":report,
                    "clean_build_proven":false}),
                )
            }
            None => Ok(
                json!({"available":false,"pending":true,"reason":"No diagnostics received; this does not mean the file has zero errors"}),
            ),
        }
    }
    pub async fn rename(
        &self,
        path: &str,
        position: Position,
        new_name: &str,
        expected_sha256: &str,
    ) -> Result<Value> {
        let content = self.sync(path).await?;
        if sha256(content.as_bytes()) != expected_sha256 {
            bail!("File changed before the rename request");
        }
        if !self.supports("renameProvider") {
            bail!("CAPABILITY_UNAVAILABLE: renameProvider");
        }
        if new_name.is_empty() || new_name.contains(['\n', '\r', '\0']) {
            bail!("Invalid new symbol name");
        }
        crate::text::offset(&content, position)?;
        if self.capabilities["renameProvider"]["prepareProvider"] == true {
            let prepared = self
                .rpc
                .request(
                    "textDocument/prepareRename",
                    json!({"textDocument":{"uri":self.uri(path)?},"position":position}),
                )
                .await?;
            if prepared.is_null() {
                bail!("Language server rejected rename at this position");
            }
        }
        self.rpc.request("textDocument/rename",json!({"textDocument":{"uri":self.uri(path)?},"position":position,"newName":new_name})).await
    }
    pub async fn hierarchy(
        &self,
        path: &str,
        position: Position,
        direction: &str,
    ) -> Result<Value> {
        let content = self.sync(path).await?;
        crate::text::offset(&content, position)?;
        if !self.supports("typeHierarchyProvider") {
            bail!("CAPABILITY_UNAVAILABLE: typeHierarchyProvider");
        }
        let method = match direction {
            "super" => "typeHierarchy/supertypes",
            "sub" => "typeHierarchy/subtypes",
            _ => bail!("direction must be super or sub"),
        };
        let items = self
            .rpc
            .request(
                "textDocument/prepareTypeHierarchy",
                json!({"textDocument":{"uri":self.uri(path)?},"position":position}),
            )
            .await?;
        let mut output = Vec::new();
        if let Some(items) = items.as_array() {
            for item in items {
                let related = self.rpc.request(method, json!({"item":item})).await?;
                output.push(json!({"item":item,"related":related}));
            }
        }
        Ok(json!({"hierarchy":output}))
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
        let content = self.sync(path).await?;
        if crate::text::offset(&content, range.start)? > crate::text::offset(&content, range.end)? {
            bail!("Reversed code action range");
        }
        if !self.supports("codeActionProvider") {
            bail!("CAPABILITY_UNAVAILABLE: codeActionProvider");
        }
        self.rpc.request("textDocument/codeAction",json!({"textDocument":{"uri":self.uri(path)?},"range":range,"context":{"diagnostics":diagnostics}})).await
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
        kind: value["kind"].as_u64().unwrap_or(0),
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
    fn nested_symbols_keep_name_paths() {
        let range = json!({"start":{"line":0,"character":0},"end":{"line":3,"character":0}});
        let item = json!({"name":"A","kind":5,"range":range,"children":[{"name":"run","kind":6,"range":range}]});
        let mut out = Vec::new();
        flatten_symbol(&item, "a.rs", "", &mut out).unwrap();
        assert_eq!(out[1].name_path, "A/run");
    }
}
