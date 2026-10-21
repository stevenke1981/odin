use crate::{Result, paths::SafeRoot, search, sha256, unix_ms};
use rusqlite::{Connection, TransactionBehavior, params};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// Version of the rebuildable lexical index (`index_files` + `index_fts`). Bump when its layout changes.
/// The index tables have their own names so binaries that predate them never write into them.
/// Databases below this version lose only the legacy `files`/`code_fts` tables; `audit` is always kept.
const SCHEMA_VERSION: i64 = 2;

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
}

struct Schema {
    version: i64,
    trace: bool,
    index: bool,
}
impl Schema {
    fn read(c: &Connection) -> Result<Self> {
        let version: i64 = c.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let trace = c
            .prepare("PRAGMA table_info(audit)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .iter()
            .any(|name| name == "trace_id");
        let index: i64 = c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('index_files','index_fts')",
            [],
            |row| row.get(0),
        )?;
        Ok(Self {
            version,
            trace,
            index: index == 2,
        })
    }
    fn is_current(&self) -> bool {
        self.version == SCHEMA_VERSION && self.trace && self.index
    }
}

/// A newer binary owns the layout of its database; refuse it untouched rather than downgrade it.
fn check_not_newer(version: i64) -> Result<()> {
    if version > SCHEMA_VERSION {
        return Err(crate::Fault::new(
            "SCHEMA_TOO_NEW",
            format!(
                "index database schema {version} is newer than supported {SCHEMA_VERSION}; it was left unchanged"
            ),
        ));
    }
    Ok(())
}

impl Store {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let this = Self {
            path: path.as_ref().to_path_buf(),
        };
        let mut c = this.connect()?;
        let before = Schema::read(&c)?;
        check_not_newer(before.version)?;
        c.execute_batch("PRAGMA journal_mode=WAL;")?;
        if before.is_current() {
            return Ok(this);
        }
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Re-read under the write lock: another process may have migrated since the check above.
        let now = Schema::read(&tx)?;
        check_not_newer(now.version)?;
        if !now.is_current() {
            if now.version < SCHEMA_VERSION {
                tx.execute_batch("DROP TABLE IF EXISTS code_fts; DROP TABLE IF EXISTS files;")?;
            }
            tx.execute_batch("CREATE TABLE IF NOT EXISTS index_files(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, hash TEXT NOT NULL);
                CREATE VIRTUAL TABLE IF NOT EXISTS index_fts USING fts5(path UNINDEXED, body, tokenize='unicode61');
                CREATE TABLE IF NOT EXISTS audit(id INTEGER PRIMARY KEY, at_ms INTEGER NOT NULL,
                    project TEXT NOT NULL, tool TEXT NOT NULL, ok INTEGER NOT NULL,
                    elapsed_ms INTEGER NOT NULL, input_hash TEXT NOT NULL, error_code TEXT);
                CREATE INDEX IF NOT EXISTS audit_time ON audit(at_ms);")?;
            if !now.trace {
                tx.execute_batch("ALTER TABLE audit ADD COLUMN trace_id TEXT;")?;
            }
            if now.version != SCHEMA_VERSION {
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
        }
        tx.commit()?;
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
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("DELETE FROM index_fts; DELETE FROM index_files;")?;
        let mut count = 0;
        let mut skipped = 0;
        {
            let mut insert_file =
                tx.prepare_cached("INSERT INTO index_files(path,hash) VALUES(?1,?2)")?;
            let mut insert_fts =
                tx.prepare_cached("INSERT INTO index_fts(rowid,path,body) VALUES(?1,?2,?3)")?;
            for path in listing.files {
                let body = match root.read(&path) {
                    Ok(b) => b,
                    Err(_) => {
                        skipped += 1;
                        continue;
                    }
                };
                let id = insert_file.insert(params![path, sha256(body.as_bytes())])?;
                insert_fts.execute(params![id, path, body])?;
                count += 1;
            }
        }
        tx.commit()?;
        Ok(
            serde_json::json!({"indexed":count,"skipped":skipped,"kind":"lexical_fts5","semantic":false}),
        )
    }
    pub fn query(&self, root: &SafeRoot, query: &str, limit: usize) -> Result<serde_json::Value> {
        let c = self.connect()?;
        // Joining on path as well as id means a row whose rowid drifted cannot pair with another file's hash.
        let mut stmt = c.prepare("SELECT index_files.path, snippet(index_fts,1,'[',']','…',24), index_files.hash
            FROM index_fts JOIN index_files ON index_files.id=index_fts.rowid AND index_files.path=index_fts.path
            WHERE index_fts MATCH ?1 ORDER BY rank LIMIT ?2")?;
        let rows = stmt.query_map(
            params![query, i64::try_from(limit).unwrap_or(i64::MAX)],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?;
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
    /// `index_fts.rowid` is always the `index_files.id` of the same file, so changes address rows by id.
    pub fn refresh(&self, root: &SafeRoot, limit: usize) -> Result<serde_json::Value> {
        let listing = search::files(root, limit)?;
        if listing.truncated {
            return Err(crate::Fault::new(
                "LIMIT_EXCEEDED",
                "Incremental scan is incomplete",
            ));
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = transaction
            .prepare("SELECT id, path, hash FROM index_files")?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    (row.get::<_, i64>(0)?, row.get::<_, String>(2)?),
                ))
            })?
            .collect::<std::result::Result<std::collections::BTreeMap<_, _>, _>>()?;
        let mut seen = std::collections::BTreeSet::new();
        let (mut updated, mut unchanged, mut skipped, mut removed) = (0, 0, 0, 0);
        {
            let mut insert_file =
                transaction.prepare_cached("INSERT INTO index_files(path,hash) VALUES(?1,?2)")?;
            let mut update_file =
                transaction.prepare_cached("UPDATE index_files SET hash=?2 WHERE id=?1")?;
            let mut delete_file =
                transaction.prepare_cached("DELETE FROM index_files WHERE id=?1")?;
            let mut insert_fts = transaction
                .prepare_cached("INSERT INTO index_fts(rowid,path,body) VALUES(?1,?2,?3)")?;
            let mut delete_fts =
                transaction.prepare_cached("DELETE FROM index_fts WHERE rowid=?1")?;
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
                if let Some((id, hash)) = old.get(&path) {
                    if *hash == digest {
                        unchanged += 1;
                        continue;
                    }
                    delete_fts.execute(params![id])?;
                    update_file.execute(params![id, digest])?;
                    insert_fts.execute(params![id, path, body])?;
                } else {
                    let id = insert_file.insert(params![path, digest])?;
                    insert_fts.execute(params![id, path, body])?;
                }
                updated += 1;
            }
            for (path, (id, _)) in &old {
                if seen.contains(path) {
                    continue;
                }
                delete_fts.execute(params![id])?;
                delete_file.execute(params![id])?;
                removed += 1;
            }
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
        let rows = stmt.query_map(
            [i64::try_from(limit.min(1000)).unwrap_or(i64::MAX)],
            |r| Ok(serde_json::json!({
                "at_ms":r.get::<_,u64>(0)?,"project":r.get::<_,String>(1)?,"tool":r.get::<_,String>(2)?,
                "ok":r.get::<_,bool>(3)?,"elapsed_ms":r.get::<_,u64>(4)?,"error_code":r.get::<_,Option<String>>(5)?,"trace_id":r.get::<_,Option<String>>(6)?
            })),
        )?;
        let entries = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(serde_json::json!({"entries":entries}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fts_count(store: &Store, term: &str) -> i64 {
        store
            .connect()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM index_fts WHERE index_fts MATCH ?1",
                [term],
                |row| row.get(0),
            )
            .unwrap()
    }
    fn table_count(store: &Store, sql: &str) -> i64 {
        store
            .connect()
            .unwrap()
            .query_row(sql, [], |row| row.get(0))
            .unwrap()
    }
    /// Pre-v2 layout: `version` 0 is the original path-keyed `files`, 1 is the id-keyed one from d4af0a8.
    fn write_legacy_db(db: &Path, version: i64) {
        let files = if version == 0 {
            "CREATE TABLE files(path TEXT PRIMARY KEY, hash TEXT NOT NULL);"
        } else {
            "CREATE TABLE files(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, hash TEXT NOT NULL);"
        };
        Connection::open(db)
            .unwrap()
            .execute_batch(&format!(
                "PRAGMA journal_mode=WAL;
                {files}
                CREATE VIRTUAL TABLE code_fts USING fts5(path UNINDEXED, body, tokenize='unicode61');
                CREATE TABLE audit(id INTEGER PRIMARY KEY, at_ms INTEGER NOT NULL,
                    project TEXT NOT NULL, tool TEXT NOT NULL, ok INTEGER NOT NULL,
                    elapsed_ms INTEGER NOT NULL, input_hash TEXT NOT NULL, error_code TEXT);
                CREATE INDEX audit_time ON audit(at_ms);
                INSERT INTO audit(at_ms,project,tool,ok,elapsed_ms,input_hash,error_code) VALUES(1,'demo','read_file',1,5,'input-digest',NULL);
                INSERT INTO files(path,hash) VALUES('a','stale');
                INSERT INTO code_fts(path,body) VALUES('a','old apple');
                PRAGMA user_version = {version};"
            ))
            .unwrap();
    }
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
    fn refresh_keeps_one_fts_row_per_file_after_changes() {
        let directory = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a"), "apple").unwrap();
        std::fs::write(directory.path().join("b"), "banana").unwrap();
        let root = SafeRoot::new(directory.path(), &[], 10000).unwrap();
        let store = Store::new(state.path().join("db")).unwrap();
        store.refresh(&root, 10).unwrap();
        std::fs::write(directory.path().join("a"), "pear").unwrap();
        std::fs::remove_file(directory.path().join("b")).unwrap();
        std::fs::write(directory.path().join("c"), "cherry").unwrap();
        store.refresh(&root, 10).unwrap();
        store.refresh(&root, 10).unwrap();
        assert_eq!(table_count(&store, "SELECT count(*) FROM index_files"), 2);
        assert_eq!(table_count(&store, "SELECT count(*) FROM index_fts"), 2);
        assert_eq!(
            table_count(
                &store,
                "SELECT count(*) FROM index_fts JOIN index_files ON index_files.id=index_fts.rowid AND index_files.path=index_fts.path"
            ),
            2
        );
        assert_eq!(fts_count(&store, "apple"), 0);
        assert_eq!(fts_count(&store, "banana"), 0);
        assert_eq!(fts_count(&store, "pear"), 1);
        assert_eq!(fts_count(&store, "cherry"), 1);
        let hits = store.query(&root, "pear", 10).unwrap();
        assert_eq!(hits["matches"][0]["path"], "a");
        assert_eq!(hits["stale_hits_omitted"], 0);
    }
    #[test]
    fn legacy_schema_migrates_and_keeps_audit() {
        for version in [0, 1] {
            let directory = tempfile::tempdir().unwrap();
            let state = tempfile::tempdir().unwrap();
            let db = state.path().join("db");
            write_legacy_db(&db, version);
            std::fs::write(directory.path().join("a"), "apple").unwrap();
            let root = SafeRoot::new(directory.path(), &[], 10000).unwrap();
            let store = Store::new(&db).unwrap();
            assert_eq!(table_count(&store, "PRAGMA user_version"), SCHEMA_VERSION);
            assert_eq!(
                table_count(
                    &store,
                    "SELECT count(*) FROM audit WHERE project='demo' AND tool='read_file'"
                ),
                1
            );
            assert_eq!(
                table_count(
                    &store,
                    "SELECT count(*) FROM pragma_table_info('audit') WHERE name='trace_id'"
                ),
                1
            );
            assert_eq!(
                table_count(
                    &store,
                    "SELECT count(*) FROM sqlite_master WHERE name IN ('files','code_fts')"
                ),
                0
            );
            assert_eq!(
                table_count(
                    &store,
                    "SELECT count(*) FROM pragma_table_info('index_files') WHERE name='id'"
                ),
                1
            );
            assert_eq!(store.refresh(&root, 10).unwrap()["updated"], 1);
            assert_eq!(fts_count(&store, "old"), 0);
            assert_eq!(table_count(&store, "SELECT count(*) FROM index_fts"), 1);
            assert_eq!(
                store.query(&root, "apple", 10).unwrap()["matches"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            Store::new(&db).unwrap();
            assert_eq!(table_count(&store, "SELECT count(*) FROM audit"), 1);
        }
    }
    #[test]
    fn old_binary_writes_to_legacy_tables_do_not_affect_query() {
        let directory = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a"), "apple").unwrap();
        let root = SafeRoot::new(directory.path(), &[], 10000).unwrap();
        let store = Store::new(state.path().join("db")).unwrap();
        store.index(&root, 10).unwrap();
        // A d4af0a8-era binary recreates the legacy tables and writes its own id/rowid pairs.
        store
            .connect()
            .unwrap()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS files(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, hash TEXT NOT NULL);
                CREATE VIRTUAL TABLE IF NOT EXISTS code_fts USING fts5(path UNINDEXED, body, tokenize='unicode61');
                INSERT INTO files(id,path,hash) VALUES(1,'b','x');
                INSERT INTO code_fts(rowid,path,body) VALUES(1,'b','apple');",
            )
            .unwrap();
        let hits = store.query(&root, "apple", 10).unwrap();
        assert_eq!(hits["matches"].as_array().unwrap().len(), 1);
        assert_eq!(hits["matches"][0]["path"], "a");
        assert_eq!(hits["stale_hits_omitted"], 0);
        assert_eq!(table_count(&store, "SELECT count(*) FROM index_fts"), 1);
    }
    #[test]
    fn query_join_requires_matching_path() {
        let directory = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a"), "pear").unwrap();
        let root = SafeRoot::new(directory.path(), &[], 10000).unwrap();
        let store = Store::new(state.path().join("db")).unwrap();
        // Row 1 belongs to `a` but its FTS rowid points at `b`'s text; the hash of `a` must not vouch for it.
        store
            .connect()
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO index_files(id,path,hash) VALUES(1,'a','{}');
                INSERT INTO index_fts(rowid,path,body) VALUES(1,'b','apple');",
                sha256("pear".as_bytes())
            ))
            .unwrap();
        let hits = store.query(&root, "apple", 10).unwrap();
        assert_eq!(hits["matches"].as_array().unwrap().len(), 0);
    }
    #[test]
    fn newer_schema_is_refused_without_touching_db() {
        let state = tempfile::tempdir().unwrap();
        let db = state.path().join("db");
        Connection::open(&db)
            .unwrap()
            .execute_batch(&format!(
                "CREATE TABLE files(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, hash TEXT NOT NULL);
                INSERT INTO files(path,hash) VALUES('a','kept');
                CREATE TABLE future(x INTEGER);
                INSERT INTO future VALUES(7);
                PRAGMA user_version = {};",
                SCHEMA_VERSION + 1
            ))
            .unwrap();
        let before = std::fs::read(&db).unwrap();
        let err = Store::new(&db)
            .err()
            .expect("a newer schema must be refused");
        assert_eq!(err.code, "SCHEMA_TOO_NEW");
        assert_eq!(std::fs::read(&db).unwrap(), before);
    }
    #[test]
    fn migrated_store_opens_while_another_process_holds_write_lock() {
        let state = tempfile::tempdir().unwrap();
        let db = state.path().join("db");
        let store = Store::new(&db).unwrap();
        let writer = Connection::open(&db).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert!(
            Store::new(&db).is_ok(),
            "an up-to-date database must not need the write lock"
        );
        writer.execute_batch("ROLLBACK").unwrap();
        assert_eq!(table_count(&store, "PRAGMA user_version"), SCHEMA_VERSION);
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
