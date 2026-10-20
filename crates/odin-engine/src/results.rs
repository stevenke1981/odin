//! Project-local, bounded result retention. Digests cover the exact stored JSON bytes.
use crate::{Engine, Project, number, string};
use odin_core::{Fault, Result, paths::atomic_write, sha256, unix_ms};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

pub const TTL_MS: u64 = 24 * 60 * 60 * 1000;
pub const MAX_RESULTS: usize = 128;
pub const MAX_STORAGE_BYTES: u64 = 128 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Metadata {
    sha256: String,
    bytes: u64,
    created_ms: u64,
    expires_ms: u64,
}

fn cleanup(directory: &Path, now: u64) -> Result<(usize, u64)> {
    let mut count = 0;
    let mut bytes = 0u64;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let filename = entry.file_name();
        let Some(id) = filename
            .to_str()
            .and_then(|name| name.strip_suffix(".meta.json"))
        else {
            continue;
        };
        let Ok(id) = uuid::Uuid::parse_str(id) else {
            continue;
        };
        let metadata: Metadata = serde_json::from_slice(&std::fs::read(entry.path())?)?;
        if metadata.expires_ms <= now {
            let data = directory.join(format!("{id}.json"));
            if data.exists() {
                std::fs::remove_file(data)?;
            }
            std::fs::remove_file(entry.path())?;
        } else {
            count += 1;
            bytes = bytes.saturating_add(metadata.bytes);
        }
    }
    Ok((count, bytes))
}

pub fn limit(engine: &Engine, project: &str, value: Value) -> Result<Value> {
    let bytes = serde_json::to_vec(&value)?;
    if bytes.len() <= engine.config.limits.max_output_bytes {
        return Ok(value);
    }
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Fault::new(
            "LIMIT_EXCEEDED",
            "Result exceeds 16 MiB; narrow the query",
        ));
    }
    let project = engine.project(project)?;
    let _guard = project
        .results_gate
        .lock()
        .map_err(|_| Fault::new("STATE_ERROR", "Result store lock poisoned"))?;
    let directory = project.state.join("results");
    let now = unix_ms();
    let (count, stored) = cleanup(&directory, now)?;
    if count >= MAX_RESULTS || stored.saturating_add(bytes.len() as u64) > MAX_STORAGE_BYTES {
        return Err(Fault::new(
            "LIMIT_EXCEEDED",
            "Unexpired project results reached retention quota",
        ));
    }
    let id = uuid::Uuid::new_v4();
    let metadata = Metadata {
        sha256: sha256(&bytes),
        bytes: bytes.len() as u64,
        created_ms: now,
        expires_ms: now.saturating_add(TTL_MS),
    };
    atomic_write(&directory.join(format!("{id}.json")), &bytes)?;
    if let Err(error) = atomic_write(
        &directory.join(format!("{id}.meta.json")),
        &serde_json::to_vec(&metadata)?,
    ) {
        let _ = std::fs::remove_file(directory.join(format!("{id}.json")));
        return Err(error);
    }
    Ok(
        json!({"truncated":true,"result_id":id,"bytes":metadata.bytes,"sha256":metadata.sha256,"created_ms":now,"expires_ms":metadata.expires_ms,
        "trace":value.get("trace"),"next":"Use result_read with this project and result_id; offsets are UTF-8 byte boundaries"}),
    )
}

pub fn read(engine: &Engine, project: &Project, args: &Value) -> Result<Value> {
    let id = uuid::Uuid::parse_str(string(args, "result_id")?)
        .map_err(|_| Fault::invalid("Invalid result ID"))?;
    let _guard = project
        .results_gate
        .lock()
        .map_err(|_| Fault::new("STATE_ERROR", "Result store lock poisoned"))?;
    let directory = project.state.join("results");
    let metadata_path = directory.join(format!("{id}.meta.json"));
    if !metadata_path.exists() {
        return Err(Fault::new(
            "RESULT_NOT_FOUND",
            "Result is not retained in this project",
        ));
    }
    let metadata: Metadata = serde_json::from_slice(&std::fs::read(&metadata_path)?)?;
    if metadata.expires_ms <= unix_ms() {
        cleanup(&directory, unix_ms())?;
        return Err(Fault::new(
            "RESULT_EXPIRED",
            "Result retention period has expired",
        ));
    }
    let bytes = std::fs::read(directory.join(format!("{id}.json")))?;
    if bytes.len() as u64 != metadata.bytes || sha256(&bytes) != metadata.sha256 {
        return Err(Fault::new(
            "CORRUPT_STATE",
            "Stored result does not match its SHA-256",
        ));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Fault::new("CORRUPT_STATE", "Stored result is not UTF-8"))?;
    let start = number(args, "offset", 0) as usize;
    let size = (number(args, "max_bytes", 16384) as usize)
        .min(engine.config.limits.max_output_bytes / 8)
        .min(32768);
    if start > text.len() || !text.is_char_boundary(start) {
        return Err(Fault::invalid(
            "Offset must be a UTF-8 boundary within the result",
        ));
    }
    let mut end = start.saturating_add(size).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == start && start < text.len() {
        end += text[start..].chars().next().map_or(0, char::len_utf8);
    }
    Ok(
        json!({"result_id":id,"text":&text[start..end],"offset":start,"next_offset":if end<text.len(){Some(end)}else{None},
        "total_bytes":metadata.bytes,"sha256":metadata.sha256,"chunk_sha256":sha256(&bytes[start..end]),"expires_ms":metadata.expires_ms}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_results_are_removed_without_evicting_fresh_results() {
        let dir = tempfile::tempdir().unwrap();
        for (expiry, tag) in [(9, "old"), (11, "new")] {
            let id = uuid::Uuid::new_v4();
            std::fs::write(dir.path().join(format!("{id}.json")), tag).unwrap();
            let metadata = Metadata {
                sha256: sha256(tag.as_bytes()),
                bytes: tag.len() as u64,
                created_ms: 0,
                expires_ms: expiry,
            };
            std::fs::write(
                dir.path().join(format!("{id}.meta.json")),
                serde_json::to_vec(&metadata).unwrap(),
            )
            .unwrap();
        }
        assert_eq!(cleanup(dir.path(), 10).unwrap(), (1, 3));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
