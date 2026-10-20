//! A WorkspaceEdit is a proposal, never permission to write to the workspace.
//! Replay its ordered operations in memory and return unique CAS transactions.
use crate::text::{TextEdit, edit};
use anyhow::{Result, anyhow, bail};
use odin_core::{paths::SafeRoot, sha256, transaction::Change};
use serde_json::Value;
use std::collections::BTreeMap;
use url::Url;

/// URL file paths omit Windows' canonical verbatim prefix (\\?\). Compare
/// against a round-tripped root before asking SafeRoot to enforce host policy.
pub(crate) fn relative_uri(root: &SafeRoot, uri: &str, allow_missing: bool) -> Result<String> {
    let url = Url::parse(uri)?;
    if url.query().is_some() || url.fragment().is_some() {
        bail!("File URI must not contain a query or fragment");
    }
    let full = url
        .to_file_path()
        .map_err(|_| anyhow!("Expected a local file URI"))?;
    let base = Url::from_directory_path(root.root())
        .map_err(|_| anyhow!("Invalid workspace URI"))?
        .to_file_path()
        .map_err(|_| anyhow!("Invalid workspace path"))?;
    let mut components = full.components();
    for expected in base.components() {
        let actual = components
            .next()
            .ok_or_else(|| anyhow!("External path is not authorized"))?;
        let same = if cfg!(windows) {
            actual
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy())
        } else {
            actual == expected
        };
        if !same {
            bail!("External path is not authorized");
        }
    }
    let relative = components.collect::<std::path::PathBuf>();
    let path = relative
        .to_str()
        .ok_or_else(|| anyhow!("Paths must be UTF-8"))?
        .replace('\\', "/");
    root.resolve(&path, allow_missing)?;
    Ok(path)
}

struct FileState {
    before: Option<String>,
    after: Option<String>,
}

pub(crate) struct Planner<'a> {
    root: &'a SafeRoot,
    snapshots: &'a BTreeMap<String, String>,
    versions: &'a BTreeMap<String, i64>,
    files: BTreeMap<String, FileState>,
    aliases: BTreeMap<String, String>,
    annotations: BTreeMap<String, Value>,
}

impl<'a> Planner<'a> {
    pub(crate) fn new(
        root: &'a SafeRoot,
        snapshots: &'a BTreeMap<String, String>,
        versions: &'a BTreeMap<String, i64>,
    ) -> Self {
        Self {
            root,
            snapshots,
            versions,
            files: BTreeMap::new(),
            aliases: BTreeMap::new(),
            annotations: BTreeMap::new(),
        }
    }

    fn path(&mut self, uri: &str) -> Result<String> {
        let path = relative_uri(self.root, uri, true)?;
        let full = self.root.resolve(&path, true)?;
        if full.exists() && !full.is_file() {
            bail!(
                "CAPABILITY_UNAVAILABLE: resource operations support regular UTF-8 files only: {path}"
            );
        }
        let key = if cfg!(windows) {
            path.to_lowercase()
        } else {
            path.clone()
        };
        if let Some(previous) = self.aliases.insert(key, path.clone())
            && previous != path
        {
            bail!("Conflicting path aliases in WorkspaceEdit: {previous} and {path}");
        }
        Ok(path)
    }

    fn file(&mut self, path: &str) -> Result<&mut FileState> {
        if !self.files.contains_key(path) {
            let full = self.root.resolve(path, true)?;
            let before = if full.exists() {
                let snapshot = self
                    .snapshots
                    .get(path)
                    .ok_or_else(|| anyhow!("No pre-request snapshot for affected file: {path}"))?;
                if self.root.read(path)? != *snapshot {
                    bail!("File changed while language server computed edits: {path}");
                }
                Some(snapshot.clone())
            } else {
                if self.snapshots.contains_key(path) {
                    bail!("File disappeared while language server computed edits: {path}");
                }
                None
            };
            self.files.insert(
                path.into(),
                FileState {
                    after: before.clone(),
                    before,
                },
            );
        }
        self.files
            .get_mut(path)
            .ok_or_else(|| anyhow!("Missing planned file"))
    }

    fn annotation(&self, value: &Value) -> Result<()> {
        if let Some(id) = value.get("annotationId") {
            let id = id
                .as_str()
                .ok_or_else(|| anyhow!("Invalid change annotation ID"))?;
            let annotation = self
                .annotations
                .get(id)
                .ok_or_else(|| anyhow!("Missing change annotation: {id}"))?;
            if !annotation.is_object()
                || annotation
                    .get("needsConfirmation")
                    .is_some_and(|v| !v.is_boolean())
            {
                bail!("Invalid change annotation: {id}");
            }
            if annotation.get("needsConfirmation").and_then(Value::as_bool) == Some(true) {
                bail!(
                    "CAPABILITY_UNAVAILABLE: change annotation requires explicit confirmation: {id}"
                );
            }
        }
        Ok(())
    }

    fn text(&mut self, uri: &str, edits: &Value, version: Option<&Value>) -> Result<()> {
        let path = self.path(uri)?;
        if let Some(version) = version.filter(|v| !v.is_null()) {
            let version = version
                .as_i64()
                .ok_or_else(|| anyhow!("Invalid document version"))?;
            if self.versions.get(&path) != Some(&version) {
                bail!("Stale LSP document version: {path}");
            }
        }
        let values = edits
            .as_array()
            .ok_or_else(|| anyhow!("Expected text edit array"))?;
        for value in values {
            self.annotation(value)?;
        }
        let edits: Vec<TextEdit> = serde_json::from_value(edits.clone())?;
        let max_bytes = self.root.max_file_bytes();
        let file = self.file(&path)?;
        let before = file
            .after
            .as_ref()
            .ok_or_else(|| anyhow!("Cannot edit missing file: {path}"))?;
        let after = edit(before, &edits)?;
        if after.len() > max_bytes {
            bail!("LIMIT_EXCEEDED: edited file exceeds host limit: {path}");
        }
        file.after = Some(after);
        Ok(())
    }

    fn flag(options: &Value, name: &str) -> Result<bool> {
        match options.get(name) {
            None => Ok(false),
            Some(Value::Bool(value)) => Ok(*value),
            _ => bail!("Invalid resource operation option: {name}"),
        }
    }

    fn resource(&mut self, change: &Value) -> Result<()> {
        self.annotation(change)?;
        let options = change
            .get("options")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if !options.is_object() {
            bail!("Resource operation options must be an object");
        }
        if Self::flag(&options, "overwrite")? {
            bail!("WorkspaceEdit overwrite is forbidden; resolve the conflict explicitly");
        }
        let uri = |name: &str| -> Result<&str> {
            change
                .get(name)
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("Missing resource URI: {name}"))
        };
        match change["kind"].as_str() {
            Some("create") => {
                let path = self.path(uri("uri")?)?;
                let file = self.file(&path)?;
                if file.after.is_some() {
                    if !Self::flag(&options, "ignoreIfExists")? {
                        bail!("CreateFile target exists: {path}");
                    }
                } else {
                    file.after = Some(String::new());
                }
            }
            Some("rename") => {
                let old = self.path(uri("oldUri")?)?;
                let new = self.path(uri("newUri")?)?;
                if old == new {
                    bail!("RenameFile source and target must differ");
                }
                let content = self
                    .file(&old)?
                    .after
                    .clone()
                    .ok_or_else(|| anyhow!("RenameFile source does not exist: {old}"))?;
                if self.file(&new)?.after.is_some() {
                    if Self::flag(&options, "ignoreIfExists")? {
                        return Ok(());
                    }
                    bail!("RenameFile target exists: {new}");
                }
                self.file(&old)?.after = None;
                self.file(&new)?.after = Some(content);
            }
            Some("delete") => {
                if Self::flag(&options, "recursive")? {
                    bail!("CAPABILITY_UNAVAILABLE: recursive DeleteFile is not supported");
                }
                let path = self.path(uri("uri")?)?;
                let file = self.file(&path)?;
                if file.after.is_none() && !Self::flag(&options, "ignoreIfNotExists")? {
                    bail!("DeleteFile target does not exist: {path}");
                }
                file.after = None;
            }
            _ => bail!("CAPABILITY_UNAVAILABLE: unknown WorkspaceEdit resource operation"),
        }
        Ok(())
    }

    pub(crate) fn plan(mut self, workspace_edit: &Value) -> Result<Vec<Change>> {
        if !workspace_edit.is_object() {
            bail!("Language server returned no valid WorkspaceEdit");
        }
        if let Some(annotations) = workspace_edit.get("changeAnnotations") {
            self.annotations = serde_json::from_value(annotations.clone())?;
        }
        // Servers must choose one representation; combining them makes order ambiguous.
        if workspace_edit.get("changes").is_some()
            && workspace_edit.get("documentChanges").is_some()
        {
            bail!("WorkspaceEdit must use either changes or documentChanges");
        }
        if let Some(changes) = workspace_edit.get("changes") {
            for (uri, edits) in changes
                .as_object()
                .ok_or_else(|| anyhow!("Expected changes object"))?
            {
                self.text(uri, edits, None)?;
            }
        }
        if let Some(changes) = workspace_edit.get("documentChanges") {
            for change in changes
                .as_array()
                .ok_or_else(|| anyhow!("Expected documentChanges array"))?
            {
                if change.get("kind").is_some() {
                    self.resource(change)?;
                } else {
                    let document = &change["textDocument"];
                    let uri = document["uri"]
                        .as_str()
                        .ok_or_else(|| anyhow!("Missing document URI"))?;
                    self.text(uri, &change["edits"], document.get("version"))?;
                }
            }
        }
        let mut changes = Vec::new();
        if !self.files.values().any(|file| file.before != file.after) {
            bail!("WorkspaceEdit contains no effective changes");
        }
        for (path, file) in self.files {
            // An existing file that ends unchanged can still be the source of
            // a rename sequence. Preserve its CAS guard in the transaction.
            if file.before.is_none() && file.after.is_none() {
                continue;
            }
            // Validate again after the entire proposal; core Transactions repeats CAS at
            // prepare/apply. The planner itself never changes files or creates parents.
            let full = self.root.resolve(&path, true)?;
            let current = if full.exists() {
                Some(self.root.read(&path)?)
            } else {
                None
            };
            if current != file.before {
                bail!("File changed while planning WorkspaceEdit: {path}");
            }
            changes.push(Change {
                path,
                expected_sha256: file.before.as_ref().map(|text| sha256(text.as_bytes())),
                content: file.after,
            });
        }
        if changes.is_empty() {
            bail!("WorkspaceEdit contains no effective changes");
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use odin_core::transaction::Transactions;
    use serde_json::json;

    fn uri(root: &SafeRoot, path: &str) -> String {
        Url::from_file_path(root.root().join(path)).unwrap().into()
    }
    fn replace(text: &str) -> Value {
        json!([{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":text}])
    }
    fn state() -> (
        tempfile::TempDir,
        SafeRoot,
        BTreeMap<String, String>,
        BTreeMap<String, i64>,
    ) {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.rs"), "alpha 😀\r\n").unwrap();
        std::fs::write(temp.path().join("dead.rs"), "gone").unwrap();
        let root = SafeRoot::new(temp.path(), &[], 1024).unwrap();
        let snapshots = BTreeMap::from([
            ("a.rs".into(), "alpha 😀\r\n".into()),
            ("dead.rs".into(), "gone".into()),
        ]);
        let versions = BTreeMap::from([("a.rs".into(), 7)]);
        (temp, root, snapshots, versions)
    }
    fn ordered(root: &SafeRoot) -> Value {
        json!({"documentChanges":[
            {"textDocument":{"uri":uri(root,"a.rs"),"version":7},"edits":replace("A")},
            {"kind":"rename","oldUri":uri(root,"a.rs"),"newUri":uri(root,"transient.rs")},
            {"textDocument":{"uri":uri(root,"transient.rs"),"version":null},"edits":replace("B")},
            {"kind":"create","uri":uri(root,"new.rs")},
            {"textDocument":{"uri":uri(root,"new.rs"),"version":null},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"newText":"fn new() {}"}]},
            {"kind":"rename","oldUri":uri(root,"transient.rs"),"newUri":uri(root,"final.rs")},
            {"kind":"delete","uri":uri(root,"dead.rs")}
        ]})
    }

    #[test]
    fn ordered_resources_are_one_guarded_core_transaction() {
        let (_temp, root, snapshots, versions) = state();
        let changes = Planner::new(&root, &snapshots, &versions)
            .plan(&ordered(&root))
            .unwrap();
        assert_eq!(changes.len(), 4);
        assert_eq!(
            changes
                .iter()
                .find(|c| c.path == "a.rs")
                .unwrap()
                .expected_sha256,
            Some(sha256(snapshots["a.rs"].as_bytes()))
        );
        assert!(
            changes
                .iter()
                .find(|c| c.path == "final.rs")
                .unwrap()
                .expected_sha256
                .is_none()
        );
        assert!(!root.root().join("final.rs").exists());
        assert_eq!(root.read("a.rs").unwrap(), snapshots["a.rs"]);
        let journal = tempfile::tempdir().unwrap();
        let transactions = Transactions::new(root.clone(), journal.path(), 100).unwrap();
        let preview = transactions.prepare(changes, None).unwrap();
        assert_eq!(preview["phase"], "draft");
        assert!(!root.root().join("final.rs").exists());
        transactions
            .apply(preview["transaction_id"].as_str().unwrap())
            .unwrap();
        assert_eq!(root.read("final.rs").unwrap(), "Blpha 😀\r\n");
        assert_eq!(root.read("new.rs").unwrap(), "fn new() {}");
        assert!(!root.root().join("a.rs").exists());
        assert!(!root.root().join("dead.rs").exists());
        assert!(!root.root().join("transient.rs").exists());
    }

    #[test]
    fn target_created_after_preview_conflicts_without_any_project_write() {
        let (_temp, root, snapshots, versions) = state();
        let changes = Planner::new(&root, &snapshots, &versions)
            .plan(&ordered(&root))
            .unwrap();
        let journal = tempfile::tempdir().unwrap();
        let transactions = Transactions::new(root.clone(), journal.path(), 100).unwrap();
        let preview = transactions.prepare(changes, None).unwrap();
        std::fs::write(root.root().join("final.rs"), "external").unwrap();
        assert!(
            transactions
                .apply(preview["transaction_id"].as_str().unwrap())
                .is_err()
        );
        assert_eq!(root.read("a.rs").unwrap(), snapshots["a.rs"]);
        assert_eq!(root.read("dead.rs").unwrap(), "gone");
        assert_eq!(root.read("final.rs").unwrap(), "external");
        assert!(!root.root().join("new.rs").exists());
    }

    #[test]
    fn snapshot_version_and_external_changes_are_all_checked() {
        let (_temp, root, snapshots, mut versions) = state();
        versions.insert("a.rs".into(), 8);
        assert!(
            Planner::new(&root, &snapshots, &versions)
                .plan(&ordered(&root))
                .unwrap_err()
                .to_string()
                .contains("Stale")
        );
        versions.insert("a.rs".into(), 7);
        std::fs::write(root.root().join("a.rs"), "external").unwrap();
        assert!(
            Planner::new(&root, &snapshots, &versions)
                .plan(&ordered(&root))
                .unwrap_err()
                .to_string()
                .contains("changed")
        );
        assert!(
            Planner::new(&root, &BTreeMap::new(), &versions)
                .plan(&ordered(&root))
                .unwrap_err()
                .to_string()
                .contains("snapshot")
        );
    }

    #[test]
    fn delete_and_recreate_keeps_original_hash() {
        let (_temp, root, snapshots, versions) = state();
        let proposal = json!({"documentChanges":[
            {"kind":"delete","uri":uri(&root,"a.rs")},
            {"kind":"create","uri":uri(&root,"a.rs")},
            {"textDocument":{"uri":uri(&root,"a.rs"),"version":null},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"newText":"replacement"}]}
        ]});
        let changes = Planner::new(&root, &snapshots, &versions)
            .plan(&proposal)
            .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].expected_sha256,
            Some(sha256(snapshots["a.rs"].as_bytes()))
        );
        assert_eq!(changes[0].content.as_deref(), Some("replacement"));
    }

    #[test]
    fn resource_conflicts_never_overwrite_existing_content() {
        let (_temp, root, snapshots, versions) = state();
        for operation in [
            json!({"kind":"create","uri":uri(&root,"a.rs")}),
            json!({"kind":"create","uri":uri(&root,"new.rs"),"options":{"overwrite":true}}),
            json!({"kind":"rename","oldUri":uri(&root,"a.rs"),"newUri":uri(&root,"dead.rs")}),
            json!({"kind":"delete","uri":uri(&root,"missing.rs")}),
            json!({"kind":"delete","uri":uri(&root,"a.rs"),"options":{"recursive":true}}),
            json!({"kind":"delete","uri":uri(&root,"a.rs"),"options":{"recursive":"false"}}),
        ] {
            assert!(
                Planner::new(&root, &snapshots, &versions)
                    .plan(&json!({"documentChanges":[operation]}))
                    .is_err()
            );
        }
        assert_eq!(root.read("a.rs").unwrap(), snapshots["a.rs"]);
        assert_eq!(root.read("dead.rs").unwrap(), snapshots["dead.rs"]);
    }

    #[test]
    fn ignored_resource_operations_remain_safe_and_ordered() {
        let (_temp, root, snapshots, versions) = state();
        let proposal = json!({"documentChanges":[
            {"kind":"create","uri":uri(&root,"a.rs"),"options":{"ignoreIfExists":true}},
            {"kind":"rename","oldUri":uri(&root,"a.rs"),"newUri":uri(&root,"dead.rs"),"options":{"ignoreIfExists":true}},
            {"kind":"delete","uri":uri(&root,"missing.rs"),"options":{"ignoreIfNotExists":true}},
            {"textDocument":{"uri":uri(&root,"a.rs"),"version":7},"edits":replace("Z")}
        ]});
        let changes = Planner::new(&root, &snapshots, &versions)
            .plan(&proposal)
            .unwrap();
        assert_eq!(
            changes
                .iter()
                .find(|c| c.path == "a.rs")
                .unwrap()
                .content
                .as_deref(),
            Some("Zlpha 😀\r\n")
        );
        assert!(changes.iter().all(|c| c.path != "missing.rs"));
    }

    #[test]
    fn annotation_confirmation_and_ambiguous_shapes_fail_closed() {
        let (_temp, root, snapshots, versions) = state();
        for proposal in [
            json!({"changes":{},"documentChanges":[]}),
            json!({"documentChanges":{}}),
            json!({"documentChanges":[{"kind":"future-operation"}]}),
            json!({"changeAnnotations":{"confirm":{"label":"Delete","needsConfirmation":true}},"documentChanges":[{"kind":"delete","uri":uri(&root,"a.rs"),"annotationId":"confirm"}]}),
            json!({"documentChanges":[{"kind":"delete","uri":uri(&root,"a.rs"),"annotationId":"missing"}]}),
        ] {
            assert!(
                Planner::new(&root, &snapshots, &versions)
                    .plan(&proposal)
                    .is_err()
            );
        }
    }

    #[test]
    fn external_protected_directory_and_missing_parent_paths_are_rejected() {
        let (_temp, root, snapshots, versions) = state();
        std::fs::create_dir(root.root().join("dir")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        for target in [
            Url::from_file_path(outside.path().join("x.rs"))
                .unwrap()
                .to_string(),
            uri(&root, ".git/config"),
            uri(&root, "dir"),
            uri(&root, "missing/child.rs"),
            "https://example.com/file.rs".into(),
            format!("{}?query", uri(&root, "new.rs")),
        ] {
            let proposal = json!({"documentChanges":[{"kind":"create","uri":target}]});
            assert!(
                Planner::new(&root, &snapshots, &versions)
                    .plan(&proposal)
                    .is_err()
            );
        }
    }

    #[test]
    fn successive_text_document_edits_use_the_prior_result() {
        let (_temp, root, snapshots, versions) = state();
        let proposal = json!({"documentChanges":[
            {"textDocument":{"uri":uri(&root,"a.rs"),"version":7},"edits":replace("two")},
            {"textDocument":{"uri":uri(&root,"a.rs"),"version":7},"edits":replace("X")}
        ]});
        let changes = Planner::new(&root, &snapshots, &versions)
            .plan(&proposal)
            .unwrap();
        assert_eq!(changes[0].content.as_deref(), Some("Xwolpha 😀\r\n"));
    }

    #[test]
    fn unchanged_existing_source_still_has_a_transaction_hash_guard() {
        let (_temp, root, snapshots, versions) = state();
        let proposal = json!({"documentChanges":[
            {"kind":"rename","oldUri":uri(&root,"a.rs"),"newUri":uri(&root,"new.rs")},
            {"kind":"create","uri":uri(&root,"a.rs")},
            {"textDocument":{"uri":uri(&root,"a.rs"),"version":null},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"newText":snapshots["a.rs"]}]}
        ]});
        let changes = Planner::new(&root, &snapshots, &versions)
            .plan(&proposal)
            .unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].path, "a.rs");
        assert!(changes[0].expected_sha256.is_some());
        let journal = tempfile::tempdir().unwrap();
        let transactions = Transactions::new(root.clone(), journal.path(), 100).unwrap();
        let preview = transactions.prepare(changes, None).unwrap();
        std::fs::write(root.root().join("a.rs"), "external").unwrap();
        assert!(
            transactions
                .apply(preview["transaction_id"].as_str().unwrap())
                .is_err()
        );
        assert!(!root.root().join("new.rs").exists());
    }
}
