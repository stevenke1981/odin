//! Optimistic, journaled file transactions. Per-file replacement is atomic; the
//! entire changeset is recoverable but NOT atomically visible across all files.
use crate::{Fault, Result, paths::{SafeRoot, atomic_write, private_dir}, sha256, unix_ms};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs::{File, OpenOptions}, path::{Path, PathBuf}};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub path: String,
    /// null means the file must not exist. Existing files require an exact hash.
    pub expected_sha256: Option<String>,
    /// null deletes the file; an empty string writes an empty file.
    pub content: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snapshot {
    path: String,
    before: Option<String>,
    after: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum Phase { Draft, Prepared, Committed, Recovered, RecoveryRequired }
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    id: String,
    created_ms: u64,
    input_hash: String,
    phase: Phase,
    files: Vec<Snapshot>,
    #[serde(default)]
    owner: Option<String>,
}
#[derive(Clone)]
pub struct Transactions {
    root: SafeRoot,
    directory: PathBuf,
    max_changes: usize,
}
impl Transactions {
    pub fn new(root: SafeRoot, directory: impl AsRef<Path>, max_changes: usize) -> Result<Self> {
        let directory = directory.as_ref().to_path_buf();
        private_dir(&directory)?; private_dir(&directory.join("keys"))?;
        Ok(Self { root, directory, max_changes })
    }
    fn lock(&self) -> Result<File> {
        let file = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(self.directory.join("lock"))?;
        file.lock_exclusive()?; Ok(file)
    }
    fn path(&self, id: &str) -> Result<PathBuf> {
        let id = uuid::Uuid::parse_str(id).map_err(|_| Fault::invalid("Invalid transaction ID"))?;
        Ok(self.directory.join(format!("{id}.json")))
    }
    fn load(&self, id: &str) -> Result<Journal> { Ok(serde_json::from_slice(&std::fs::read(self.path(id)?)?)?) }
    fn save(&self, journal: &Journal) -> Result<()> {
        atomic_write(&self.path(&journal.id)?, &serde_json::to_vec_pretty(journal)?)
    }
    fn current(&self, path: &str) -> Result<Option<String>> {
        let full = self.root.resolve(path, true)?;
        if full.exists() { Ok(Some(self.root.read(path)?)) } else { Ok(None) }
    }
    fn digest(text: &Option<String>) -> Option<String> { text.as_ref().map(|s| sha256(s.as_bytes())) }
    fn write(&self, path: &str, text: &Option<String>) -> Result<()> {
        let full = self.root.resolve(path, true)?;
        match text {
            Some(content) => atomic_write(&full, content.as_bytes()),
            None if full.exists() => {
                std::fs::remove_file(&full)?;
                #[cfg(unix)]
                if let Some(parent)=full.parent(){File::open(parent)?.sync_all()?;}
                Ok(())
            },
            None => Ok(()),
        }
    }
    fn journals(&self) -> Result<Vec<Journal>> {
        let mut output = Vec::new();
        for entry in std::fs::read_dir(&self.directory)? {
            let path = entry?.path();
            if path.extension().is_some_and(|x| x == "json") {
                output.push(serde_json::from_slice::<Journal>(&std::fs::read(path)?)?);
            }
        }
        output.sort_by_key(|j| j.created_ms); Ok(output)
    }
    fn pending(&self) -> Result<bool> {
        Ok(self.journals()?.iter().any(|j| matches!(j.phase, Phase::Prepared | Phase::RecoveryRequired)))
    }
    /// Produces a durable immutable changeset; never edits project files.
    pub fn prepare(&self, changes: Vec<Change>, request_id: Option<&str>) -> Result<serde_json::Value> {
        self.prepare_scoped(changes, request_id, None)
    }
    /// Bind a shared-storage transaction to its initiating project. Ownership
    /// is part of the immutable journal and the idempotency input digest.
    pub fn prepare_scoped(&self, changes: Vec<Change>, request_id: Option<&str>, owner: Option<&str>) -> Result<serde_json::Value> {
        let _lock = self.lock()?;
        if changes.is_empty() || changes.len() > self.max_changes { return Err(Fault::invalid("Empty or oversized changeset")); }
        let input_hash = sha256(&serde_json::to_vec(&(&changes, owner))?);
        let key_path = request_id.map(|key| self.directory.join("keys").join(format!("{}.json", sha256(key.as_bytes()))));
        if let Some(path) = key_path.as_ref().filter(|p| p.exists()) {
            let existing: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
            if existing["input_hash"] != input_hash { return Err(Fault::conflict("Idempotency key reused with different input")); }
            let id = existing["id"].as_str().ok_or_else(|| Fault::new("CORRUPT_STATE", "Invalid idempotency record"))?;
            return self.preview_unlocked(&self.load(id)?);
        }
        if self.pending()? { return Err(Fault::new("RECOVERY_REQUIRED", "Recover pending transactions before preparing more writes")); }
        let mut seen = BTreeSet::new(); let mut snapshots = Vec::new();
        for change in changes {
            let full = self.root.resolve(&change.path, true)?;
            if !seen.insert(full) { return Err(Fault::invalid("Duplicate target in changeset")); }
            if change.content.as_ref().is_some_and(|s| s.len() > self.root.max_file_bytes()) {
                return Err(Fault::new("LIMIT_EXCEEDED", "Replacement exceeds file limit"));
            }
            let before = self.current(&change.path)?;
            if Self::digest(&before) != change.expected_sha256 { return Err(Fault::conflict(format!("Version mismatch: {}", change.path))); }
            if before.is_none() && change.content.is_none() { return Err(Fault::invalid("Cannot delete a missing file")); }
            snapshots.push(Snapshot { path: change.path, before, after: change.content });
        }
        let journal = Journal { id: uuid::Uuid::new_v4().to_string(), created_ms: unix_ms(), input_hash,
            phase: Phase::Draft, files: snapshots, owner: owner.map(str::to_owned) };
        self.save(&journal)?;
        if let Some(path) = key_path {
            atomic_write(&path, &serde_json::to_vec(&serde_json::json!({"id":journal.id,"input_hash":journal.input_hash}))?)?;
        }
        self.preview_unlocked(&journal)
    }
    fn preview_unlocked(&self, journal: &Journal) -> Result<serde_json::Value> {
        let files: Vec<_> = journal.files.iter().map(|s| {
            let before = s.before.as_deref().unwrap_or(""); let after = s.after.as_deref().unwrap_or("");
            let diff = similar::TextDiff::from_lines(before, after).unified_diff().context_radius(3)
                .header(&format!("a/{}",s.path), &format!("b/{}",s.path)).to_string();
            serde_json::json!({"path":s.path,"before_sha256":Self::digest(&s.before),"after_sha256":Self::digest(&s.after),"diff":diff})
        }).collect();
        Ok(serde_json::json!({"transaction_id":journal.id,"phase":journal.phase,"files":files,
            "requires_apply":journal.phase==Phase::Draft,"atomic_visibility":"per_file_only","journaled_recovery":true}))
    }
    pub fn preview(&self, id: &str) -> Result<serde_json::Value> {
        let _lock = self.lock()?; self.preview_unlocked(&self.load(id)?)
    }
    pub fn owner_and_paths(&self, id: &str) -> Result<(Option<String>, Vec<String>)> {
        let _lock = self.lock()?;
        let journal = self.load(id)?;
        Ok((journal.owner, journal.files.into_iter().map(|file| file.path).collect()))
    }
    pub fn list(&self) -> Result<serde_json::Value> {
        let _lock = self.lock()?;
        Ok(serde_json::json!({"transactions":self.journals()?.iter().map(|j|serde_json::json!({
            "id":j.id,"phase":j.phase,"created_ms":j.created_ms,"files":j.files.len()
        })).collect::<Vec<_>>()}))
    }
    /// Validate the whole changeset before the first write. Never overwrite an
    /// externally modified file to make rollback appear successful.
    pub fn apply(&self, id: &str) -> Result<serde_json::Value> {
        let _lock = self.lock()?; let mut journal = self.load(id)?;
        if journal.phase == Phase::Committed { return Ok(serde_json::json!({"transaction_id":id,"phase":"committed","idempotent_replay":true})); }
        if journal.phase != Phase::Draft { return Err(Fault::conflict("Only a draft transaction can be applied")); }
        if self.pending()? { return Err(Fault::new("RECOVERY_REQUIRED", "Recover pending transactions first")); }
        for file in &journal.files {
            if self.current(&file.path)? != file.before { return Err(Fault::conflict(format!("File changed after preview: {}",file.path))); }
        }
        journal.phase = Phase::Prepared; self.save(&journal)?;
        for file in &journal.files {
            let result = (|| -> Result<()> {
                if self.current(&file.path)? != file.before { return Err(Fault::conflict(format!("Concurrent external modification: {}",file.path))); }
                self.write(&file.path, &file.after)
            })();
            if let Err(error) = result {
                let rollback = self.restore(&journal);
                journal.phase = if rollback.is_ok() { Phase::Recovered } else { Phase::RecoveryRequired };
                self.save(&journal)?;
                return Err(Fault::new("TRANSACTION_FAILED", format!("{error}; rollback_status={:?}", journal.phase)));
            }
        }
        journal.phase = Phase::Committed; self.save(&journal)?;
        Ok(serde_json::json!({"transaction_id":id,"phase":"committed","files":journal.files.len()}))
    }
    fn restore(&self, journal: &Journal) -> Result<()> {
        // Preflight all files before undoing any: an unrelated writer wins.
        for file in &journal.files {
            let now = self.current(&file.path)?;
            if now != file.before && now != file.after { return Err(Fault::conflict(format!("Recovery would overwrite external edit: {}",file.path))); }
        }
        for file in journal.files.iter().rev() {
            let now = self.current(&file.path)?;
            if now == file.before { continue; }
            if now != file.after { return Err(Fault::conflict("External edit during recovery")); }
            self.write(&file.path, &file.before)?;
        }
        Ok(())
    }
    pub fn recover(&self) -> Result<serde_json::Value> {
        let _lock = self.lock()?; let mut recovered = Vec::new();
        for mut journal in self.journals()? {
            if !matches!(journal.phase,Phase::Prepared|Phase::RecoveryRequired) { continue; }
            match self.restore(&journal) {
                Ok(()) => { journal.phase = Phase::Recovered; self.save(&journal)?; recovered.push(journal.id); },
                Err(error) => { journal.phase = Phase::RecoveryRequired; self.save(&journal)?; return Err(error); },
            }
        }
        Ok(serde_json::json!({"recovered":recovered}))
    }
    pub fn rollback(&self, id: &str) -> Result<serde_json::Value> {
        let journal = { let _lock = self.lock()?; self.load(id)? };
        if journal.phase != Phase::Committed { return Err(Fault::conflict("Only committed transactions can be undone")); }
        let changes = journal.files.iter().map(|f| Change { path:f.path.clone(), expected_sha256:Self::digest(&f.after),content:f.before.clone() }).collect();
        let key = format!("undo-{id}");
        let preview = self.prepare(changes,Some(&key))?;
        let undo_id = preview["transaction_id"].as_str().ok_or_else(|| Fault::new("CORRUPT_STATE","Missing transaction id"))?;
        let mut result = self.apply(undo_id)?;
        result["undo_of"] = id.into(); Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir,tempfile::TempDir,Transactions) {
        let root=tempfile::tempdir().unwrap();let state=tempfile::tempdir().unwrap();
        let tx=Transactions::new(SafeRoot::new(root.path(),&[],10000).unwrap(),state.path(),32).unwrap();
        (root,state,tx)
    }
    #[test] fn prepare_does_not_write_and_apply_is_idempotent() {
        let (root,_s,tx)=setup(); let p=tx.prepare(vec![Change{path:"x".into(),expected_sha256:None,content:Some("hello".into())}],Some("key")).unwrap();
        assert!(!root.path().join("x").exists());let id=p["transaction_id"].as_str().unwrap();
        tx.apply(id).unwrap();assert_eq!(std::fs::read_to_string(root.path().join("x")).unwrap(),"hello");
        assert_eq!(tx.apply(id).unwrap()["idempotent_replay"],true);
    }
    #[test] fn conflict_before_commit_has_no_partial_write() {
        let(root,_s,tx)=setup();std::fs::write(root.path().join("b"),"old").unwrap();
        let p=tx.prepare(vec![Change{path:"a".into(),expected_sha256:None,content:Some("one".into())},Change{path:"b".into(),expected_sha256:Some(sha256(b"old")),content:Some("new".into())}],None).unwrap();
        std::fs::write(root.path().join("b"),"external").unwrap();assert!(tx.apply(p["transaction_id"].as_str().unwrap()).is_err());assert!(!root.path().join("a").exists());
    }
    #[test] fn undo_create_and_edit() {
        let(root,_s,tx)=setup();std::fs::write(root.path().join("b"),"old").unwrap();
        let p=tx.prepare(vec![Change{path:"a".into(),expected_sha256:None,content:Some("one".into())},Change{path:"b".into(),expected_sha256:Some(sha256(b"old")),content:Some("new".into())}],None).unwrap();
        let id=p["transaction_id"].as_str().unwrap();tx.apply(id).unwrap();tx.rollback(id).unwrap();
        assert!(!root.path().join("a").exists());assert_eq!(std::fs::read_to_string(root.path().join("b")).unwrap(),"old");
    }
    #[test] fn idempotency_key_cannot_change_meaning() {
        let(_r,_s,tx)=setup();let c=Change{path:"x".into(),expected_sha256:None,content:Some("a".into())};tx.prepare(vec![c.clone()],Some("k")).unwrap();
        let mut other=c;other.content=Some("b".into());assert!(tx.prepare(vec![other],Some("k")).is_err());
    }
    #[test] fn recovery_restores_a_simulated_interrupted_commit() {
        let(root,_s,tx)=setup();let p=tx.prepare(vec![Change{path:"x".into(),expected_sha256:None,content:Some("a".into())}],None).unwrap();
        let mut j=tx.load(p["transaction_id"].as_str().unwrap()).unwrap();j.phase=Phase::Prepared;tx.save(&j).unwrap();std::fs::write(root.path().join("x"),"a").unwrap();
        tx.recover().unwrap();assert!(!root.path().join("x").exists());
    }
    #[test] fn recovery_refuses_to_destroy_an_external_change() {
        let(root,_s,tx)=setup();let p=tx.prepare(vec![Change{path:"x".into(),expected_sha256:None,content:Some("a".into())}],None).unwrap();
        let mut j=tx.load(p["transaction_id"].as_str().unwrap()).unwrap();j.phase=Phase::Prepared;tx.save(&j).unwrap();std::fs::write(root.path().join("x"),"external").unwrap();
        assert!(tx.recover().is_err());assert_eq!(std::fs::read_to_string(root.path().join("x")).unwrap(),"external");
    }
}
