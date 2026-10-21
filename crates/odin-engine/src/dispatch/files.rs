//! Read-only project file listing, reading, pattern search and index tools.

use crate::{Engine, Project, boolean, number_usize, optional_string, string};
use odin_core::{Fault, Result, search, sha256};
use serde_json::{Value, json};
use std::sync::Arc;

impl Engine {
    pub(crate) async fn dispatch_files(
        self: &Arc<Self>,
        p: &Arc<Project>,
        tool: &str,
        a: &Value,
    ) -> Result<Value> {
        let limit = &self.config.limits;
        match tool {
            "list_dir" => {
                let listing = search::files(&p.root, limit.max_files)?;
                let prefix = optional_string(a, "relative_path").unwrap_or(".");
                p.root.resolve(prefix, false)?;
                let normalized = prefix.trim_end_matches('/');
                let recursive = boolean(a, "recursive", true);
                let files = listing
                    .files
                    .into_iter()
                    .filter(|path| {
                        let rel = if normalized == "." {
                            Some(path.as_str())
                        } else {
                            path.strip_prefix(&format!("{normalized}/"))
                        };
                        rel.is_some_and(|rel| recursive || !rel.contains('/'))
                    })
                    .collect::<Vec<_>>();
                Ok(json!({"files":files,"truncated":listing.truncated,"kind":"files_only"}))
            }
            "find_file" => {
                let mask = globset::Glob::new(string(a, "file_mask")?)
                    .map_err(|e| Fault::invalid(e.to_string()))?
                    .compile_matcher();
                let listing = search::files(&p.root, limit.max_files)?;
                Ok(
                    json!({"files":listing.files.into_iter().filter(|path|mask.is_match(path)).collect::<Vec<_>>(),"truncated":listing.truncated}),
                )
            }
            "read_file" => {
                let path = string(a, "relative_path")?;
                let text = p.root.read(path)?;
                let hash = sha256(text.as_bytes());
                let start = number_usize(a, "start_line", 1);
                let lines = text.split_inclusive('\n').collect::<Vec<_>>();
                let end = number_usize(a, "end_line", lines.len());
                if start == 0 || start > lines.len() + 1 || end > lines.len() || end + 1 < start {
                    return Err(Fault::invalid("Invalid inclusive line range"));
                }
                let content = if end < start {
                    String::new()
                } else {
                    lines[start - 1..end].concat()
                };
                Ok(
                    json!({"relative_path":path,"sha256":hash,"content":content,"start_line":start,"end_line":end,"total_lines":lines.len(),"line_base":1}),
                )
            }
            "search_for_pattern" => search::pattern(
                &p.root,
                string(a, "pattern")?,
                limit.max_files,
                limit.max_matches,
            ),
            "index_build" => p.store.index(&p.root, limit.max_files),
            "index_query" => {
                p.store.refresh(&p.root, limit.max_files)?;
                p.store
                    .query(&p.root, string(a, "query")?, number_usize(a, "limit", 20))
            }
            _ => Err(Fault::new(
                "UNKNOWN_TOOL",
                format!("No dispatcher for {tool}"),
            )),
        }
    }
}
