use crate::{Result, paths::SafeRoot, search, sha256, unix_ms};
use rusqlite::{Connection, params};
use std::{path::{Path, PathBuf}, time::Duration};

#[derive(Clone)]
pub struct Store { path: PathBuf }
impl Store {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let this = Self { path: path.as_ref().to_path_buf() };
        let c = this.connect()?;
        c.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY, hash TEXT NOT NULL);
            CREATE VIRTUAL TABLE IF NOT EXISTS code_fts USING fts5(path UNINDEXED, body, tokenize='unicode61');
            CREATE TABLE IF NOT EXISTS audit(id INTEGER PRIMARY KEY, at_ms INTEGER NOT NULL,
                project TEXT NOT NULL, tool TEXT NOT NULL, ok INTEGER NOT NULL,
                elapsed_ms INTEGER NOT NULL, input_hash TEXT NOT NULL, error_code TEXT);
            CREATE INDEX IF NOT EXISTS audit_time ON audit(at_ms);")?;
        Ok(this)
    }
    fn connect(&self) -> Result<Connection> {
        let c = Connection::open(&self.path)?; c.busy_timeout(Duration::from_secs(5))?; Ok(c)
    }
    pub fn index(&self, root: &SafeRoot, limit: usize) -> Result<serde_json::Value> {
        let listing = search::files(root, limit)?;
        if listing.truncated { return Err(crate::Fault::new("LIMIT_EXCEEDED", "Index would be incomplete; raise max_files")); }
        let mut c = self.connect()?; let tx = c.transaction()?;
        tx.execute_batch("DELETE FROM files; DELETE FROM code_fts;")?;
        let mut count = 0; let mut skipped = 0;
        for path in listing.files {
            let body = match root.read(&path) { Ok(b) => b, Err(_) => { skipped += 1; continue; } };
            tx.execute("INSERT INTO files(path,hash) VALUES(?1,?2)", params![path, sha256(body.as_bytes())])?;
            tx.execute("INSERT INTO code_fts(path,body) VALUES(?1,?2)", params![path, body])?;
            count += 1;
        }
        tx.commit()?;
        Ok(serde_json::json!({"indexed":count,"skipped":skipped,"kind":"lexical_fts5","semantic":false}))
    }
    pub fn query(&self, root: &SafeRoot, query: &str, limit: usize) -> Result<serde_json::Value> {
        let c = self.connect()?;
        let mut stmt = c.prepare("SELECT code_fts.path, snippet(code_fts,1,'[',']','…',24), files.hash
            FROM code_fts JOIN files ON files.path=code_fts.path WHERE code_fts MATCH ?1 ORDER BY rank LIMIT ?2")?;
        let rows = stmt.query_map(params![query, limit as i64], |row| Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?)))?;
        let mut hits = Vec::new(); let mut stale = 0;
        for row in rows {
            let (path, snippet, hash) = row?;
            match root.read(&path) {
                Ok(body) if sha256(body.as_bytes()) == hash => hits.push(serde_json::json!({"path":path,"snippet":snippet,"sha256":hash})),
                _ => stale += 1,
            }
        }
        Ok(serde_json::json!({"matches":hits,"stale_hits_omitted":stale,"index_is_snapshot":true,"reindex_after_changes":true}))
    }
    pub fn audit(&self, project: &str, tool: &str, ok: bool, elapsed: u64, input_hash: &str, code: Option<&str>) -> Result<()> {
        self.connect()?.execute("INSERT INTO audit(at_ms,project,tool,ok,elapsed_ms,input_hash,error_code) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![unix_ms(),project,tool,ok,elapsed,input_hash,code])?;
        Ok(())
    }
    pub fn recent(&self, limit: usize) -> Result<serde_json::Value> {
        let c = self.connect()?;
        let mut stmt = c.prepare("SELECT at_ms,project,tool,ok,elapsed_ms,error_code FROM audit ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit.min(1000) as i64], |r| Ok(serde_json::json!({
            "at_ms":r.get::<_,u64>(0)?,"project":r.get::<_,String>(1)?,"tool":r.get::<_,String>(2)?,
            "ok":r.get::<_,bool>(3)?,"elapsed_ms":r.get::<_,u64>(4)?,"error_code":r.get::<_,Option<String>>(5)?
        })))?;
        let entries = rows.collect::<std::result::Result<Vec<_>,_>>()?;
        Ok(serde_json::json!({"entries":entries}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn fts_excludes_stale_contents() {
        let d = tempfile::tempdir().unwrap(); let state = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("x"), "apple orange").unwrap();
        let root = SafeRoot::new(d.path(), &[], 1000).unwrap(); let store = Store::new(state.path().join("db")).unwrap();
        store.index(&root,10).unwrap(); assert_eq!(store.query(&root,"apple",10).unwrap()["matches"].as_array().unwrap().len(),1);
        std::fs::write(d.path().join("x"), "pear").unwrap();
        assert_eq!(store.query(&root,"apple",10).unwrap()["stale_hits_omitted"],1);
    }
}
