//! Human-readable Markdown, explicit mem: references, and journaled updates.
//! Storage is host-controlled; no vector database or automatic prompt injection.
use crate::{
    Fault, Result,
    paths::{SafeRoot, private_dir},
    sha256,
    transaction::{Change, Transactions},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryBundle {
    format: String,
    schema_version: u32,
    project: String,
    notes: Vec<ExportedNote>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportedNote {
    name: String,
    content: String,
    sha256: String,
}

#[derive(Clone)]
pub struct Memories {
    root: SafeRoot,
    tx: Transactions,
    projects: BTreeSet<String>,
    writable_projects: BTreeSet<String>,
    global_writable: bool,
    read_only: Vec<regex::Regex>,
    max_changes: usize,
}
impl Memories {
    pub fn new(
        state: &Path,
        projects: BTreeSet<String>,
        writable_projects: BTreeSet<String>,
        global_writable: bool,
        patterns: &[String],
        max_bytes: usize,
        max_changes: usize,
    ) -> Result<Self> {
        #[cfg(windows)]
        if projects
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
            .len()
            != projects.len()
        {
            return Err(Fault::invalid("Memory project IDs collide on Windows"));
        }
        let dir = state.join("memories");
        private_dir(&dir.join("global"))?;
        for id in &projects {
            crate::config::validate_id(id)?;
            private_dir(&dir.join("projects").join(id))?;
        }
        let root = SafeRoot::new(&dir, &[], max_bytes)?;
        let tx = Transactions::new(root.clone(), state.join("memory_transactions"), max_changes)?;
        Ok(Self {
            root,
            tx,
            projects,
            writable_projects,
            global_writable,
            read_only: patterns
                .iter()
                .map(|p| {
                    regex::RegexBuilder::new(p)
                        .case_insensitive(cfg!(windows))
                        .build()
                })
                .collect::<std::result::Result<_, _>>()?,
            max_changes,
        })
    }
    fn name_path(&self, project: &str, name: &str) -> Result<String> {
        if !self.projects.contains(project) {
            return Err(Fault::invalid("Unknown memory project"));
        }
        validate_name(name)?;
        let relative = if let Some(name) = name.strip_prefix("global/") {
            validate_name(name)?;
            format!("global/{name}.md")
        } else {
            format!("projects/{project}/{name}.md")
        };
        #[cfg(windows)]
        {
            let canonical =
                crate::config::canonical_storage_path(&self.root.root().join(&relative))?;
            let actual = canonical
                .strip_prefix(self.root.root())
                .map_err(|_| Fault::denied("Memory escaped storage"))?
                .to_string_lossy()
                .replace('\\', "/");
            if actual != relative {
                return Err(Fault::denied("Use the canonical existing memory name"));
            }
        }
        Ok(relative)
    }
    fn check_write(&self, project: &str, name: &str) -> Result<()> {
        if self.read_only.iter().any(|r| r.is_match(name)) {
            return Err(Fault::denied("Memory is read-only by policy"));
        }
        if name.starts_with("global/") {
            if !self.global_writable {
                return Err(Fault::denied("Global memory writes are disabled"));
            }
        } else if !self.writable_projects.contains(project) {
            return Err(Fault::denied("Project memory writes are disabled"));
        }
        Ok(())
    }
    fn ensure_parent(&self, relative: &str) -> Result<()> {
        let path = self.root.root().join(relative);
        let parent = path
            .parent()
            .ok_or_else(|| Fault::invalid("Memory has no parent"))?;
        private_dir(parent)?;
        self.root.resolve(relative, true)?;
        Ok(())
    }
    fn all_paths(&self) -> Result<Vec<String>> {
        let mut output = Vec::new();
        let mut queue = vec![self.root.root().to_path_buf()];
        while let Some(dir) = queue.pop() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    queue.push(entry.path());
                } else if kind.is_file() && entry.path().extension().is_some_and(|s| s == "md") {
                    output.push(self.root.relative(&entry.path())?);
                    if output.len() > 10_000 {
                        return Err(Fault::new("LIMIT_EXCEEDED", "Too many memory files"));
                    }
                }
            }
        }
        output.sort();
        Ok(output)
    }
    fn path_name(&self, path: &str) -> Result<(String, String)> {
        if let Some(name) = path
            .strip_prefix("global/")
            .and_then(|s| s.strip_suffix(".md"))
        {
            return Ok((String::new(), format!("global/{name}")));
        }
        let tail = path
            .strip_prefix("projects/")
            .ok_or_else(|| Fault::invalid("Unknown memory namespace"))?;
        let (project, name) = tail
            .split_once('/')
            .ok_or_else(|| Fault::invalid("Malformed memory path"))?;
        Ok((
            project.into(),
            name.strip_suffix(".md")
                .ok_or_else(|| Fault::invalid("Expected Markdown"))?
                .into(),
        ))
    }
    pub fn list(&self, project: &str) -> Result<Value> {
        if !self.projects.contains(project) {
            return Err(Fault::invalid("Unknown project"));
        }
        let mut values = Vec::new();
        for path in self.all_paths()? {
            let (scope, name) = self.path_name(&path)?;
            if scope.is_empty() || scope == project {
                values.push(
                    json!({"name":name,"read_only":self.check_write(project,&name).is_err()}),
                );
            }
        }
        Ok(json!({"memories":values,"contents_loaded":false}))
    }
    pub fn read(&self, project: &str, name: &str) -> Result<Value> {
        let content = self.root.read(&self.name_path(project, name)?)?;
        Ok(json!({"name":name,"sha256":sha256(content.as_bytes()),"content":content}))
    }
    /// Versioned data only: no YAML tags, templates, commands or executable config.
    pub fn export(&self, project: &str) -> Result<Value> {
        self.name_path(project, "validation")?;
        let mut notes = Vec::new();
        let mut bytes = 0usize;
        for path in self.all_paths()? {
            let (scope, name) = self.path_name(&path)?;
            if !scope.is_empty() && scope != project {
                continue;
            }
            let content = self.root.read(&path)?;
            bytes = bytes
                .checked_add(content.len())
                .filter(|bytes| *bytes <= 16 * 1024 * 1024)
                .ok_or_else(|| Fault::new("LIMIT_EXCEEDED", "Memory export exceeds 16 MiB"))?;
            notes.push(ExportedNote {
                name,
                sha256: sha256(content.as_bytes()),
                content,
            });
        }
        Ok(serde_json::to_value(MemoryBundle {
            format: "odin-memory-bundle".into(),
            schema_version: 1,
            project: project.into(),
            notes,
        })?)
    }
    /// Create-only import. All digests and references are validated before the
    /// journal is prepared; existing notes are never silently merged/overwritten.
    pub fn import(&self, project: &str, bundle: &Value) -> Result<Value> {
        self.name_path(project, "validation")?;
        let bundle: MemoryBundle = serde_json::from_value(bundle.clone())?;
        if bundle.format != "odin-memory-bundle" || bundle.schema_version != 1 {
            return Err(Fault::new(
                "SCHEMA_UNSUPPORTED",
                "Unsupported memory bundle format/version",
            ));
        }
        if bundle.notes.len() > self.max_changes {
            return Err(Fault::new("LIMIT_EXCEEDED", "Too many imported notes"));
        }
        let mut names = BTreeSet::new();
        let mut changes = Vec::new();
        for note in &bundle.notes {
            self.check_write(project, &note.name)?;
            let path = self.name_path(project, &note.name)?;
            if !names.insert(note.name.clone()) {
                return Err(Fault::invalid("Duplicate memory name in bundle"));
            }
            if sha256(note.content.as_bytes()) != note.sha256 {
                return Err(Fault::new(
                    "INTEGRITY_ERROR",
                    "Memory bundle digest mismatch",
                ));
            }
            if self.root.root().join(&path).exists() {
                return Err(Fault::conflict("Import destination memory exists"));
            }
            changes.push(Change {
                path,
                expected_sha256: None,
                content: Some(note.content.clone()),
            });
        }
        for note in &bundle.notes {
            for cap in reference_regex()?.captures_iter(&note.content) {
                let target = &cap[1];
                if note.name.starts_with("global/") && !target.starts_with("global/") {
                    return Err(Fault::invalid(
                        "Global memories cannot contain project-local references",
                    ));
                }
                if !names.contains(target) && self.read(project, target).is_err() {
                    return Err(Fault::invalid(format!(
                        "Unresolved imported memory reference: {target}"
                    )));
                }
            }
        }
        for change in &changes {
            self.ensure_parent(&change.path)?;
        }
        self.tx.prepare_scoped(changes, None, Some(project))
    }
    /// A scope move refuses references that cannot be expressed in the current
    /// mem: name syntax, instead of rebinding them to an unrelated local note.
    pub fn move_scope(
        &self,
        project: &str,
        name: &str,
        destination_project: &str,
        destination_name: &str,
        expected: &str,
    ) -> Result<Value> {
        self.check_write(project, name)?;
        self.check_write(destination_project, destination_name)?;
        let source = self.name_path(project, name)?;
        let destination = self.name_path(destination_project, destination_name)?;
        if source == destination {
            return Err(Fault::invalid(
                "Memory source and destination are identical",
            ));
        }
        if self.root.root().join(&destination).exists() {
            return Err(Fault::conflict("Destination memory exists"));
        }
        let content = self.root.read(&source)?;
        if sha256(content.as_bytes()) != expected {
            return Err(Fault::conflict("Memory changed before move"));
        }
        let source_global = name.starts_with("global/");
        let target_global = destination_name.starts_with("global/");
        let changes_scope = target_global || project != destination_project;
        if changes_scope {
            for cap in reference_regex()?.captures_iter(&content) {
                let target = &cap[1];
                if target != name && !target.starts_with("global/") {
                    return Err(Fault::new(
                        "NEED_REVIEW",
                        "Outgoing local references cannot be moved across memory scopes",
                    ));
                }
            }
        }
        let mut changes = vec![
            Change {
                path: source.clone(),
                expected_sha256: Some(expected.into()),
                content: None,
            },
            Change {
                path: destination.clone(),
                expected_sha256: None,
                content: Some(rewrite_refs(
                    &content,
                    name,
                    destination_name,
                    self.root.max_file_bytes(),
                )?),
            },
        ];
        for path in self.all_paths()? {
            if path == source {
                continue;
            }
            let (scope, referring_name) = self.path_name(&path)?;
            if !source_global && scope != project {
                continue;
            }
            let before = self.root.read(&path)?;
            if !reference_regex()?
                .captures_iter(&before)
                .any(|cap| &cap[1] == name)
            {
                continue;
            }
            let after = rewrite_refs(&before, name, destination_name, self.root.max_file_bytes())?;
            if !target_global && (scope.is_empty() || scope != destination_project) {
                return Err(Fault::new(
                    "NEED_REVIEW",
                    "Incoming references cannot cross project-local memory scopes",
                ));
            }
            let check_project = if scope.is_empty() { project } else { &scope };
            self.check_write(check_project, &referring_name)?;
            check_change_budget(&changes, &before, &after, self.max_changes)?;
            changes.push(Change {
                path,
                expected_sha256: Some(sha256(before.as_bytes())),
                content: Some(after),
            });
        }
        self.ensure_parent(&destination)?;
        self.tx.prepare_scoped(changes, None, Some(project))
    }
    fn authorize_transaction(&self, project: &str, id: &str) -> Result<()> {
        self.name_path(project, "validation")?;
        let (owner, paths) = self.tx.owner_and_paths(id)?;
        if owner.as_deref() != Some(project) {
            return Err(Fault::denied(
                "Memory transaction belongs to another project or operator",
            ));
        }
        for path in paths {
            let (scope, name) = self.path_name(&path)?;
            self.check_write(if scope.is_empty() { project } else { &scope }, &name)?;
        }
        Ok(())
    }
    pub fn preview(&self, project: &str, id: &str) -> Result<Value> {
        self.authorize_transaction(project, id)?;
        self.tx.preview(id)
    }
    pub fn apply(&self, project: &str, id: &str) -> Result<Value> {
        self.authorize_transaction(project, id)?;
        self.tx.apply(id)
    }
    pub fn write(
        &self,
        project: &str,
        name: &str,
        content: Option<String>,
        expected: Option<String>,
    ) -> Result<Value> {
        self.check_write(project, name)?;
        let path = self.name_path(project, name)?;
        self.ensure_parent(&path)?;
        let preview = self.tx.prepare(
            vec![Change {
                path,
                expected_sha256: expected,
                content,
            }],
            None,
        )?;
        self.tx.apply(
            preview["transaction_id"]
                .as_str()
                .ok_or_else(|| Fault::new("CORRUPT_STATE", "Missing id"))?,
        )
    }
    /// Rename within a scope. Every affected writable Markdown reference is
    /// rewritten in the same journal. Read-only referring notes cause refusal.
    pub fn rename(&self, project: &str, old: &str, new: &str, expected: &str) -> Result<Value> {
        self.check_write(project, old)?;
        self.check_write(project, new)?;
        if old.starts_with("global/") != new.starts_with("global/") {
            return Err(Fault::invalid(
                "Cross-scope memory moves are explicit export/import operations",
            ));
        }
        if old == new {
            return Err(Fault::invalid("Names are identical"));
        }
        let old_path = self.name_path(project, old)?;
        let new_path = self.name_path(project, new)?;
        self.ensure_parent(&new_path)?;
        if self.root.resolve(&new_path, true)?.exists() {
            return Err(Fault::conflict("Destination memory exists"));
        }
        let content = self.root.read(&old_path)?;
        if sha256(content.as_bytes()) != expected {
            return Err(Fault::conflict("Memory changed before rename"));
        }
        let mut changes = vec![
            Change {
                path: old_path.clone(),
                expected_sha256: Some(expected.into()),
                content: None,
            },
            Change {
                path: new_path,
                expected_sha256: None,
                content: Some(rewrite_refs(
                    &content,
                    old,
                    new,
                    self.root.max_file_bytes(),
                )?),
            },
        ];
        for path in self.all_paths()? {
            if path == old_path {
                continue;
            }
            let (scope, name) = self.path_name(&path)?;
            // Local memory names only have meaning within their own project.
            if !old.starts_with("global/") && scope != project {
                continue;
            }
            let before = self.root.read(&path)?;
            let after = rewrite_refs(&before, old, new, self.root.max_file_bytes())?;
            if before == after {
                continue;
            }
            let check_project = if scope.is_empty() {
                project
            } else {
                scope.as_str()
            };
            self.check_write(check_project, &name)?;
            check_change_budget(&changes, &before, &after, self.max_changes)?;
            changes.push(Change {
                path,
                expected_sha256: Some(sha256(before.as_bytes())),
                content: Some(after),
            });
        }
        let preview = self.tx.prepare(changes, None)?;
        self.tx.apply(
            preview["transaction_id"]
                .as_str()
                .ok_or_else(|| Fault::new("CORRUPT_STATE", "Missing id"))?,
        )
    }
    pub fn check(&self, project: &str) -> Result<Value> {
        let refs = reference_regex()?;
        let mut missing = Vec::new();
        for path in self.all_paths()? {
            let (scope, name) = self.path_name(&path)?;
            if !scope.is_empty() && scope != project {
                continue;
            }
            let body = self.root.read(&path)?;
            for cap in refs.captures_iter(&body) {
                let target = &cap[1];
                // A global note may reference only global names deterministically.
                if scope.is_empty() && !target.starts_with("global/") {
                    missing.push(json!({"memory":name,"reference":target,"reason":"local_reference_in_global_memory"}));
                    continue;
                }
                if self
                    .name_path(project, target)
                    .and_then(|p| self.root.read(&p))
                    .is_err()
                {
                    missing.push(json!({"memory":name,"reference":target,"reason":"unresolved"}));
                }
            }
        }
        Ok(json!({"valid":missing.is_empty(),"unresolved":missing}))
    }
    pub fn search(&self, project: &str, pattern: &str, limit: usize) -> Result<Value> {
        if pattern.len() > 4096 {
            return Err(Fault::invalid("Pattern too long"));
        }
        let re = regex::Regex::new(pattern)?;
        let mut results = Vec::new();
        let mut truncated = false;
        for path in self.all_paths()? {
            let (scope, name) = self.path_name(&path)?;
            if !scope.is_empty() && scope != project {
                continue;
            }
            let text = self.root.read(&path)?;
            if re.is_match(&name) || re.is_match(&text) {
                if results.len() >= limit {
                    truncated = true;
                    break;
                }
                results.push(json!({"name":name,"sha256":sha256(text.as_bytes())}));
            }
        }
        Ok(json!({"matches":results,"truncated":truncated,"kind":"regex_not_vector"}))
    }
    pub fn recover(&self) -> Result<Value> {
        self.tx.recover()
    }
    pub fn directory(&self) -> PathBuf {
        self.root.root().to_path_buf()
    }
}
fn reference_regex() -> Result<regex::Regex> {
    Ok(regex::Regex::new(r"mem:([\p{L}\p{N}_/.-]+)")?)
}
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 240
        || name == "global"
        || name.starts_with('/')
        || name.ends_with('/')
        || name
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || "_-./".contains(c))
    {
        return Err(Fault::invalid("Invalid memory name"));
    }
    Ok(())
}
fn rewrite_refs(content: &str, old: &str, new: &str, limit: usize) -> Result<String> {
    let re = reference_regex()?;
    let mut size = content.len();
    for captures in re.captures_iter(content) {
        if &captures[1] == old {
            size = size
                .checked_sub(captures[0].len())
                .and_then(|size| size.checked_add(4 + new.len()))
                .filter(|size| *size <= limit)
                .ok_or_else(|| {
                    Fault::new(
                        "LIMIT_EXCEEDED",
                        "Memory reference expansion exceeds file limit",
                    )
                })?;
        }
    }
    if size > limit {
        return Err(Fault::new("LIMIT_EXCEEDED", "Memory exceeds file limit"));
    }
    Ok(re
        .replace_all(content, |caps: &regex::Captures<'_>| {
            if &caps[1] == old {
                format!("mem:{new}")
            } else {
                caps[0].to_owned()
            }
        })
        .into_owned())
}
fn check_change_budget(
    changes: &[Change],
    before: &str,
    after: &str,
    max_changes: usize,
) -> Result<()> {
    if changes.len() >= max_changes {
        return Err(Fault::new("LIMIT_EXCEEDED", "Too many memory changes"));
    }
    let current = changes
        .iter()
        .try_fold(0usize, |sum, change| {
            sum.checked_add(change.content.as_ref().map_or(0, String::len))
        })
        .and_then(|bytes| bytes.checked_add(before.len()))
        .and_then(|bytes| bytes.checked_add(after.len()));
    if current.is_none_or(|bytes| bytes > crate::transaction::MAX_JOURNAL_BYTES) {
        return Err(Fault::new(
            "LIMIT_EXCEEDED",
            "Memory changes exceed journal budget",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_replacement_is_exact() {
        assert_eq!(
            rewrite_refs(
                "`mem:core` mem:core_extra mem:core/api",
                "core",
                "main",
                1000
            )
            .unwrap(),
            "`mem:main` mem:core_extra mem:core/api"
        );
    }
    #[test]
    fn reference_expansion_is_rejected_before_allocating_the_result() {
        assert_eq!(
            rewrite_refs("mem:x mem:x mem:x", "x", &"long".repeat(20), 64)
                .unwrap_err()
                .code,
            "LIMIT_EXCEEDED"
        );
        assert_eq!(
            rewrite_refs("mem:longname", "longname", "x", 8).unwrap(),
            "mem:x"
        );
    }
    #[test]
    fn memory_name_traversal_is_rejected() {
        for n in ["../secret", "/x", "a//b", "global", "a/../b", "a\\b"] {
            assert!(validate_name(n).is_err(), "{n}");
        }
        assert!(validate_name("架構/模組").is_ok());
    }
    #[test]
    fn memory_rename_updates_references() {
        let d = tempfile::tempdir().unwrap();
        let ps = BTreeSet::from(["demo".into()]);
        let m = Memories::new(d.path(), ps.clone(), ps, false, &[], 10000, 32).unwrap();
        m.write("demo", "core", Some("Architecture".into()), None)
            .unwrap();
        m.write("demo", "index", Some("Read `mem:core`".into()), None)
            .unwrap();
        m.rename("demo", "core", "design", &sha256(b"Architecture"))
            .unwrap();
        assert_eq!(
            m.read("demo", "index").unwrap()["content"],
            "Read `mem:design`"
        );
        assert_eq!(m.check("demo").unwrap()["valid"], true);
    }
    #[test]
    fn global_writes_require_separate_permission() {
        let d = tempfile::tempdir().unwrap();
        let ps = BTreeSet::from(["demo".into()]);
        let m = Memories::new(d.path(), ps.clone(), ps, false, &[], 10000, 32).unwrap();
        assert!(m.write("demo", "global/x", Some("x".into()), None).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn windows_memory_aliases_cannot_bypass_readonly_or_reference_policy() {
        let directory = tempfile::tempdir().unwrap();
        let projects = BTreeSet::from(["demo".into()]);
        let memories = Memories::new(
            directory.path(),
            projects.clone(),
            projects.clone(),
            false,
            &["^policy$".into(), "^index$".into()],
            10000,
            16,
        )
        .unwrap();
        let notes = directory.path().join("memories/projects/demo");
        std::fs::write(notes.join("policy.md"), "immutable").unwrap();
        std::fs::write(notes.join("INDEX.md"), "mem:core").unwrap();
        std::fs::write(notes.join("core.md"), "body").unwrap();
        for content in [Some("changed".into()), None] {
            assert_eq!(
                memories
                    .write("demo", "POLICY", content, Some(sha256(b"immutable")))
                    .unwrap_err()
                    .code,
                "PERMISSION_DENIED"
            );
        }
        assert_eq!(
            memories
                .rename("demo", "core", "design", &sha256(b"body"))
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
        assert!(memories.read("demo", "core").is_ok());
        assert_eq!(
            memories
                .write(
                    "demo",
                    "CORE",
                    Some("changed".into()),
                    Some(sha256(b"body"))
                )
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
        assert_eq!(
            Memories::new(
                directory.path(),
                BTreeSet::from(["demo".into(), "DEMO".into()]),
                projects,
                false,
                &[],
                10000,
                16
            )
            .err()
            .unwrap()
            .code,
            "INVALID_ARGUMENT"
        );
    }
    #[cfg(windows)]
    #[test]
    fn new_windows_notes_cannot_use_existing_parent_case_aliases() {
        let directory = tempfile::tempdir().unwrap();
        let projects = BTreeSet::from(["demo".into()]);
        let memories = Memories::new(
            directory.path(),
            projects.clone(),
            projects,
            false,
            &[],
            10000,
            16,
        )
        .unwrap();
        std::fs::create_dir(directory.path().join("memories/projects/demo/design")).unwrap();
        memories
            .write("demo", "core", Some("body".into()), None)
            .unwrap();
        assert_eq!(
            memories
                .rename("demo", "core", "Design/new_note", &sha256(b"body"))
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
        assert_eq!(
            memories
                .write("demo", "Design/new_note", Some("new".into()), None)
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
        assert!(memories.read("demo", "core").is_ok());
        memories
            .write("demo", "design/new_note", Some("new".into()), None)
            .unwrap();
        assert_eq!(
            memories.read("demo", "design/new_note").unwrap()["content"],
            "new"
        );
    }
    fn two_projects(state: &Path, global: bool) -> Memories {
        let projects = BTreeSet::from(["one".into(), "two".into()]);
        Memories::new(state, projects.clone(), projects, global, &[], 10000, 32).unwrap()
    }
    #[test]
    fn memory_bundle_roundtrip_is_previewed_and_owned() {
        let state = tempfile::tempdir().unwrap();
        let memories = two_projects(state.path(), false);
        memories
            .write("one", "core", Some("Architecture".into()), None)
            .unwrap();
        memories
            .write("one", "index", Some("See mem:core".into()), None)
            .unwrap();
        let bundle = memories.export("one").unwrap();
        let preview = memories.import("two", &bundle).unwrap();
        let id = preview["transaction_id"].as_str().unwrap();
        assert!(memories.read("two", "core").is_err());
        assert_eq!(
            memories.preview("one", id).unwrap_err().code,
            "PERMISSION_DENIED"
        );
        assert_eq!(
            memories.apply("one", id).unwrap_err().code,
            "PERMISSION_DENIED"
        );
        memories.apply("two", id).unwrap();
        assert_eq!(
            memories.read("two", "core").unwrap()["content"],
            "Architecture"
        );
        assert_eq!(memories.check("two").unwrap()["valid"], true);
        assert_eq!(
            memories.import("two", &bundle).unwrap_err().code,
            "CONFLICT"
        );
    }
    #[test]
    fn memory_import_rejects_digest_tampering_and_unresolved_refs() {
        let state = tempfile::tempdir().unwrap();
        let memories = two_projects(state.path(), false);
        let mut bundle = json!({"format":"odin-memory-bundle","schema_version":1,"project":"one", "notes":[{"name":"core","content":"tampered","sha256":sha256(b"original")}]});
        assert_eq!(
            memories.import("two", &bundle).unwrap_err().code,
            "INTEGRITY_ERROR"
        );
        bundle["notes"][0]["content"] = "mem:missing".into();
        bundle["notes"][0]["sha256"] = sha256(b"mem:missing").into();
        assert_eq!(
            memories.import("two", &bundle).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
        assert!(memories.read("two", "core").is_err());
    }
    #[test]
    fn scope_move_updates_references_in_one_cas_journal() {
        let state = tempfile::tempdir().unwrap();
        let memories = two_projects(state.path(), true);
        memories
            .write("one", "core", Some("Architecture".into()), None)
            .unwrap();
        memories
            .write("one", "index", Some("See mem:core".into()), None)
            .unwrap();
        let preview = memories
            .move_scope(
                "one",
                "core",
                "one",
                "global/core",
                &sha256(b"Architecture"),
            )
            .unwrap();
        assert!(memories.read("one", "core").is_ok());
        memories
            .apply("one", preview["transaction_id"].as_str().unwrap())
            .unwrap();
        assert!(memories.read("one", "core").is_err());
        assert_eq!(
            memories.read("one", "index").unwrap()["content"],
            "See mem:global/core"
        );
        assert_eq!(
            memories.read("two", "global/core").unwrap()["content"],
            "Architecture"
        );
    }
    #[test]
    fn scope_move_never_rebinds_cross_project_local_references() {
        let state = tempfile::tempdir().unwrap();
        let memories = two_projects(state.path(), true);
        memories
            .write("one", "core", Some("Architecture".into()), None)
            .unwrap();
        memories
            .write("one", "index", Some("See mem:core".into()), None)
            .unwrap();
        assert_eq!(
            memories
                .move_scope("one", "core", "two", "core", &sha256(b"Architecture"))
                .unwrap_err()
                .code,
            "NEED_REVIEW"
        );
        assert_eq!(
            memories.read("one", "index").unwrap()["content"],
            "See mem:core"
        );
        assert!(memories.read("two", "core").is_err());
    }
    #[test]
    fn prepared_import_detects_external_writes() {
        let state = tempfile::tempdir().unwrap();
        let memories = two_projects(state.path(), false);
        memories
            .write("one", "core", Some("Architecture".into()), None)
            .unwrap();
        let preview = memories
            .import("two", &memories.export("one").unwrap())
            .unwrap();
        memories
            .write("two", "core", Some("third party".into()), None)
            .unwrap();
        assert_eq!(
            memories
                .apply("two", preview["transaction_id"].as_str().unwrap())
                .unwrap_err()
                .code,
            "CONFLICT"
        );
        assert_eq!(
            memories.read("two", "core").unwrap()["content"],
            "third party"
        );
    }
}
