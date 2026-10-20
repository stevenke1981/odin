use crate::{Result, paths::SafeRoot, search, sha256, unix_ms};
use rusqlite::{Connection, params};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
}
impl Store {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let this = Self {
            path: path.as_ref().to_path_buf(),
        };
        let c = this.connect()?;
        c.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY, hash TEXT NOT NULL);
            CREATE VIRTUAL TABLE IF NOT EXISTS code_fts USING fts5(path UNINDEXED, body, tokenize='unicode61');
            CREATE TABLE IF NOT EXISTS audit(id INTEGER PRIMARY KEY, at_ms INTEGER NOT NULL,
                project TEXT NOT NULL, tool TEXT NOT NULL, ok INTEGER NOT NULL,
                elapsed_ms INTEGER NOT NULL, input_hash TEXT NOT NULL, error_code TEXT);
            CREATE INDEX IF NOT EXISTS audit_time ON audit(at_ms);")?;
        let has_trace: bool = c
            .prepare("PRAGMA table_info(audit)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .iter()
            .any(|name| name == "trace_id");
        if !has_trace {
            c.execute_batch("ALTER TABLE audit ADD COLUMN trace_id TEXT;")?;
        }
        Ok(this)
    }
    fn connect(&self) -> Result<Connection> {
        let c = Connection::open(&self.path)?;
        c.busy_timeout(Duration::from_secs(5))?;
        Ok(c)
    }
    pub fn index(&self, root: &SafeRoot, limit: usize) -> Result<serde_json::Value> {
        let listing = search::files(root, limit)?;
        if listing.truncated {
            return Err(crate::Fault::new(
                "LIMIT_EXCEEDED",
                "Index would be incomplete; raise max_files",
            ));
        }
        let mut c = self.connect()?;
        let tx = c.transaction()?;
        tx.execute_batch("DELETE FROM files; DELETE FROM code_fts;")?;
        let mut count = 0;
        let mut skipped = 0;
        for path in listing.files {
            let body = match root.read(&path) {
                Ok(b) => b,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            tx.execute(
                "INSERT INTO files(path,hash) VALUES(?1,?2)",
                params![path, sha256(body.as_bytes())],
            )?;
            tx.execute(
                "INSERT INTO code_fts(path,body) VALUES(?1,?2)",
                params![path, body],
            )?;
            count += 1;
        }
        tx.commit()?;
        Ok(
            serde_json::json!({"indexed":count,"skipped":skipped,"kind":"lexical_fts5","semantic":false}),
        )
    }
    pub fn query(&self, root: &SafeRoot, query: &str, limit: usize) -> Result<serde_json::Value> {
        let c = self.connect()?;
        let mut stmt = c.prepare("SELECT code_fts.path, snippet(code_fts,1,'[',']','…',24), files.hash
            FROM code_fts JOIN files ON files.path=code_fts.path WHERE code_fts MATCH ?1 ORDER BY rank LIMIT ?2")?;
        let rows = stmt.query_map(params![query, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut hits = Vec::new();
        let mut stale = 0;
        for row in rows {
            let (path, snippet, hash) = row?;
            match root.read(&path) {
                Ok(body) if sha256(body.as_bytes()) == hash => {
                    hits.push(serde_json::json!({"path":path,"snippet":snippet,"sha256":hash}))
                }
                _ => stale += 1,
            }
        }
        Ok(
            serde_json::json!({"matches":hits,"stale_hits_omitted":stale,"index_is_snapshot":true,"reindex_after_changes":true}),
        )
    }
    /// Reconcile a bounded live scan, touching FTS rows only when hashes change.
    /// This is demand-driven refresh, not a background filesystem watcher.
    pub fn refresh(&self, root: &SafeRoot, limit: usize) -> Result<serde_json::Value> {
        let listing = search::files(root, limit)?;
        if listing.truncated {
            return Err(crate::Fault::new(
                "LIMIT_EXCEEDED",
                "Incremental scan is incomplete",
            ));
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let old = transaction
            .prepare("SELECT path, hash FROM files")?
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<std::collections::BTreeMap<_, _>, _>>()?;
        let mut seen = std::collections::BTreeSet::new();
        let (mut updated, mut unchanged, mut skipped, mut removed) = (0, 0, 0, 0);
        for path in listing.files {
            let body = match root.read(&path) {
                Ok(body) => body,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let digest = sha256(body.as_bytes());
            seen.insert(path.clone());
            if old.get(&path) == Some(&digest) {
                unchanged += 1;
                continue;
            }
            transaction.execute("DELETE FROM code_fts WHERE path=?1", [&path])?;
            transaction.execute(
                "INSERT INTO code_fts(path,body) VALUES(?1,?2)",
                params![path, body],
            )?;
            transaction.execute("INSERT INTO files(path,hash) VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET hash=excluded.hash", params![path, digest])?;
            updated += 1;
        }
        for path in old.keys().filter(|path| !seen.contains(*path)) {
            transaction.execute("DELETE FROM code_fts WHERE path=?1", [path])?;
            transaction.execute("DELETE FROM files WHERE path=?1", [path])?;
            removed += 1;
        }
        transaction.commit()?;
        Ok(
            serde_json::json!({"updated":updated,"unchanged":unchanged,"removed":removed,"skipped":skipped,"kind":"incremental_lexical_fts5","watcher":false}),
        )
    }
    pub fn audit(
        &self,
        project: &str,
        tool: &str,
        ok: bool,
        elapsed: u64,
        input_hash: &str,
        code: Option<&str>,
    ) -> Result<()> {
        self.connect()?.execute("INSERT INTO audit(at_ms,project,tool,ok,elapsed_ms,input_hash,error_code) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![unix_ms(),project,tool,ok,elapsed,input_hash,code])?;
        Ok(())
    }
    pub fn audit_with_trace(
        &self,
        project: &str,
        tool: &str,
        ok: bool,
        elapsed: u64,
        input_hash: &str,
        trace: (&str, Option<&str>),
    ) -> Result<()> {
        let (trace_id, code) = trace;
        self.connect()?.execute("INSERT INTO audit(at_ms,project,tool,ok,elapsed_ms,input_hash,error_code,trace_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![unix_ms(),project,tool,ok,elapsed,input_hash,code,trace_id])?;
        Ok(())
    }
    pub fn recent(&self, limit: usize) -> Result<serde_json::Value> {
        let c = self.connect()?;
        let mut stmt = c.prepare("SELECT at_ms,project,tool,ok,elapsed_ms,error_code,trace_id FROM audit ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit.min(1000) as i64], |r| Ok(serde_json::json!({
            "at_ms":r.get::<_,u64>(0)?,"project":r.get::<_,String>(1)?,"tool":r.get::<_,String>(2)?,
            "ok":r.get::<_,bool>(3)?,"elapsed_ms":r.get::<_,u64>(4)?,"error_code":r.get::<_,Option<String>>(5)?,"trace_id":r.get::<_,Option<String>>(6)?
        })))?;
        let entries = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(serde_json::json!({"entries":entries}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fts_excludes_stale_contents() {
        let d = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("x"), "apple orange").unwrap();
        let root = SafeRoot::new(d.path(), &[], 1000).unwrap();
        let store = Store::new(state.path().join("db")).unwrap();
        store.index(&root, 10).unwrap();
        assert_eq!(
            store.query(&root, "apple", 10).unwrap()["matches"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        std::fs::write(d.path().join("x"), "pear").unwrap();
        assert_eq!(
            store.query(&root, "apple", 10).unwrap()["stale_hits_omitted"],
            1
        );
    }
    #[test]
    fn incremental_refresh_updates_additions_changes_and_deletions() {
        let directory = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a"), "apple").unwrap();
        std::fs::write(directory.path().join("b"), "banana").unwrap();
        let root = SafeRoot::new(directory.path(), &[], 10000).unwrap();
        let store = Store::new(state.path().join("db")).unwrap();
        assert_eq!(store.refresh(&root, 10).unwrap()["updated"], 2);
        assert_eq!(store.refresh(&root, 10).unwrap()["unchanged"], 2);
        std::fs::write(directory.path().join("a"), "pear").unwrap();
        std::fs::remove_file(directory.path().join("b")).unwrap();
        std::fs::write(directory.path().join("c"), "cherry").unwrap();
        let result = store.refresh(&root, 10).unwrap();
        assert_eq!(result["updated"], 2);
        assert_eq!(result["removed"], 1);
        assert_eq!(
            store.query(&root, "pear", 10).unwrap()["matches"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store.query(&root, "banana", 10).unwrap()["matches"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
    #[test]
    fn trace_audit_contains_metadata_without_source() {
        let state = tempfile::tempdir().unwrap();
        let store = Store::new(state.path().join("db")).unwrap();
        store
            .audit_with_trace(
                "demo",
                "read_file",
                true,
                12,
                "input-digest",
                ("trace-123", None),
            )
            .unwrap();
        let recent = store.recent(10).unwrap();
        assert_eq!(recent["entries"][0]["trace_id"], "trace-123");
        assert_eq!(recent["entries"][0]["elapsed_ms"], 12);
        assert!(recent["entries"][0].get("content").is_none());
    }
}
