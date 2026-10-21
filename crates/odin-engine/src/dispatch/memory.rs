//! Project and global memory operations.

use crate::{Engine, Project, number_usize, optional_string, replace, string};
use odin_core::{Fault, Result};
use serde_json::{Value, json};
use std::sync::Arc;

impl Engine {
    pub(crate) async fn dispatch_memory(
        self: &Arc<Self>,
        p: &Arc<Project>,
        tool: &str,
        a: &Value,
    ) -> Result<Value> {
        let limit = &self.config.limits;
        match tool {
            "list_memories" => self.memories.list(&p.id),
            "read_memory" => self.memories.read(&p.id, string(a, "name")?),
            "write_memory" => self.memories.write(
                &p.id,
                string(a, "name")?,
                Some(string(a, "content")?.into()),
                optional_string(a, "expected_sha256").map(str::to_string),
            ),
            "delete_memory" => self.memories.write(
                &p.id,
                string(a, "name")?,
                None,
                Some(string(a, "expected_sha256")?.into()),
            ),
            "edit_memory" => {
                let name = string(a, "name")?;
                let memory = self.memories.read(&p.id, name)?;
                if memory["sha256"] != string(a, "expected_sha256")? {
                    return Err(Fault::conflict("Memory version mismatch"));
                }
                let (after, count) = replace(
                    memory["content"]
                        .as_str()
                        .ok_or_else(|| Fault::invalid("Memory body missing"))?,
                    string(a, "needle")?,
                    string(a, "replacement")?,
                    false,
                    (
                        limit.max_file_bytes,
                        limit.max_matches,
                        number_usize(a, "expected_matches", 1),
                    ),
                )?;
                if count != number_usize(a, "expected_matches", 1) {
                    return Err(Fault::conflict("Unexpected replacement count"));
                }
                self.memories.write(
                    &p.id,
                    name,
                    Some(after),
                    Some(string(a, "expected_sha256")?.into()),
                )
            }
            "rename_memory" => self.memories.rename(
                &p.id,
                string(a, "old_name")?,
                string(a, "new_name")?,
                string(a, "expected_sha256")?,
            ),
            "memory_export" => Ok(json!({"bundle":self.memories.export(&p.id)?})),
            "memory_import" => self.memories.import(&p.id, &a["bundle"]),
            "memory_move_scope" => self.memories.move_scope(
                &p.id,
                string(a, "name")?,
                string(a, "destination_project")?,
                string(a, "destination_name")?,
                string(a, "expected_sha256")?,
            ),
            "memory_transaction_preview" => {
                self.memories.preview(&p.id, string(a, "transaction_id")?)
            }
            "memory_transaction_apply" => self.memories.apply(&p.id, string(a, "transaction_id")?),
            "check_memories" => self.memories.check(&p.id),
            "search_memories" => {
                self.memories
                    .search(&p.id, string(a, "pattern")?, limit.max_matches)
            }
            "recover_memories" => {
                if self.projects.values().any(|p| !p.config.allow_write)
                    || !self.config.allow_global_memory_write
                {
                    return Err(Fault::denied(
                        "Global memory journal recovery is operator-only unless all configured scopes are writable",
                    ));
                }
                self.memories.recover()
            }
            _ => Err(Fault::new(
                "UNKNOWN_TOOL",
                format!("No dispatcher for {tool}"),
            )),
        }
    }
}
