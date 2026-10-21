//! Native source edits. Each produces a transaction; nothing is written directly.

use crate::{
    Engine, Project, backend_error, boolean, line_edit, number, number_usize, occurrences,
    optional_string, read_expected, replace, string,
};
use odin_core::transaction::Change;
use odin_core::{Fault, Result, search, sha256};
use odin_protocol::text::{Range, TextEdit, edit};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;

impl Engine {
    pub(crate) async fn dispatch_edits(
        self: &Arc<Self>,
        p: &Arc<Project>,
        tool: &str,
        a: &Value,
    ) -> Result<Value> {
        let limit = &self.config.limits;
        match tool {
            "create_text_file" => p.transactions.prepare(
                vec![Change {
                    path: string(a, "relative_path")?.into(),
                    expected_sha256: optional_string(a, "expected_sha256").map(str::to_string),
                    content: Some(string(a, "content")?.into()),
                }],
                optional_string(a, "request_id"),
            ),
            "replace_content" => {
                let path = string(a, "relative_path")?;
                let before = read_expected(p, path, string(a, "expected_sha256")?)?;
                let (after, count) = replace(
                    &before,
                    string(a, "needle")?,
                    string(a, "replacement")?,
                    boolean(a, "regex", false),
                    (
                        limit.max_file_bytes,
                        limit.max_matches,
                        number_usize(a, "expected_matches", 1),
                    ),
                )?;
                if count != number_usize(a, "expected_matches", 1) {
                    return Err(Fault::conflict(format!(
                        "Expected {} matches, found {count}",
                        number(a, "expected_matches", 1)
                    )));
                }
                p.transactions.prepare(
                    vec![Change {
                        path: path.into(),
                        expected_sha256: Some(sha256(before.as_bytes())),
                        content: Some(after),
                    }],
                    None,
                )
            }
            "replacement_preview" => {
                occurrences::prepare(p, a, true, limit.max_matches, limit.max_output_bytes)
            }
            "replace_in_files" => {
                occurrences::prepare(p, a, false, limit.max_matches, limit.max_output_bytes)
            }
            "replace_lines" | "delete_lines" | "insert_at_line" => {
                let path = string(a, "relative_path")?;
                let before = read_expected(p, path, string(a, "expected_sha256")?)?;
                let start = number_usize(a, "start_line", 1);
                let end = number_usize(a, "end_line", start);
                let after = line_edit(
                    &before,
                    start,
                    end,
                    optional_string(a, "content").unwrap_or(""),
                    tool == "insert_at_line",
                )?;
                p.transactions.prepare(
                    vec![Change {
                        path: path.into(),
                        expected_sha256: Some(sha256(before.as_bytes())),
                        content: Some(after),
                    }],
                    None,
                )
            }
            "replace_symbol_body"
            | "insert_before_symbol"
            | "insert_after_symbol"
            | "safe_delete_symbol" => {
                let path = string(a, "relative_path")?;
                let before = read_expected(p, path, string(a, "expected_sha256")?)?;
                let client = self.lsp_for(p, path).await?;
                let symbol = client
                    .symbol(path, string(a, "name_path")?)
                    .await
                    .map_err(backend_error)?;
                if p.root.read(path)? != before {
                    return Err(Fault::conflict("File changed while resolving symbol"));
                }
                if tool == "safe_delete_symbol" {
                    let references = client
                        .at(path, symbol.selection_range.start, "references")
                        .await
                        .map_err(backend_error)?;
                    if !references.is_null() && !references.as_array().is_some_and(Vec::is_empty) {
                        return Err(Fault::conflict("Symbol still has static references"));
                    }
                }
                let range = match tool {
                    "insert_before_symbol" => Range {
                        start: symbol.range.start,
                        end: symbol.range.start,
                    },
                    "insert_after_symbol" => Range {
                        start: symbol.range.end,
                        end: symbol.range.end,
                    },
                    _ => symbol.range,
                };
                let after = edit(
                    &before,
                    &[TextEdit {
                        range,
                        new_text: optional_string(a, "body").unwrap_or("").into(),
                    }],
                )
                .map_err(backend_error)?;
                let mut result = p.transactions.prepare(
                    vec![Change {
                        path: path.into(),
                        expected_sha256: Some(sha256(before.as_bytes())),
                        content: Some(after),
                    }],
                    None,
                )?;
                if tool == "safe_delete_symbol" {
                    result["safety_scope"] = json!(
                        "static_lsp_references_only; reflection/dynamic consumers may not be visible"
                    );
                }
                Ok(result)
            }
            "rename_symbol" => {
                let path = string(a, "relative_path")?;
                let target_before = p.root.read(path)?;
                let client = self.lsp_for(p, path).await?;
                let symbol = client
                    .symbol(path, string(a, "name_path")?)
                    .await
                    .map_err(backend_error)?;
                let listing = search::files(&p.root, limit.max_files)?;
                if listing.truncated {
                    return Err(Fault::new(
                        "LIMIT_EXCEEDED",
                        "Rename snapshot would omit files",
                    ));
                }
                let mut snapshots = BTreeMap::new();
                let mut bytes = 0;
                for file in listing.files {
                    if let Ok(text) = p.root.read(&file) {
                        bytes += text.len();
                        if bytes > 64 * 1024 * 1024 {
                            return Err(Fault::new(
                                "LIMIT_EXCEEDED",
                                "Native rename snapshot exceeds 64 MiB",
                            ));
                        }
                        snapshots.insert(file, text);
                    }
                }
                if snapshots.get(path) != Some(&target_before) {
                    return Err(Fault::conflict(
                        "Target changed while locating the rename symbol",
                    ));
                }
                let workspace_edit = client
                    .rename(
                        path,
                        symbol.selection_range.start,
                        string(a, "new_name")?,
                        &sha256(target_before.as_bytes()),
                    )
                    .await
                    .map_err(backend_error)?;
                let changes = client
                    .changes(&workspace_edit, &snapshots)
                    .await
                    .map_err(backend_error)?;
                p.transactions.prepare(changes, None)
            }
            _ => Err(Fault::new(
                "UNKNOWN_TOOL",
                format!("No dispatcher for {tool}"),
            )),
        }
    }
}
