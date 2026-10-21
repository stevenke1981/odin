//! Native LSP IDE tools. Backend edits are proposals and never server commands.
use crate::{
    Engine, Project, backend_error, boolean, number, optional_string, read_expected, string,
};
use odin_core::{Fault, Result, sha256};
use odin_protocol::text::{Position, Range, end_position};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

pub async fn dispatch(
    engine: &Arc<Engine>,
    project: &Arc<Project>,
    tool: &str,
    args: &Value,
) -> Result<Value> {
    if tool == "format_document" && args.get("formatter_profile").is_some() {
        return crate::formatter::prepare(engine, project, args).await;
    }
    if tool == "workspace_symbols" {
        let language = string(args, "language")?;
        let language = project
            .config
            .languages
            .iter()
            .find(|entry| entry.id == language)
            .cloned()
            .ok_or_else(|| {
                Fault::new("MISSING_BACKEND", "Language is not configured by the host")
            })?;
        let query = string(args, "query")?;
        if query.len() > 4096 {
            return Err(Fault::invalid("Workspace query exceeds 4096 bytes"));
        }
        let client = engine.lsp_language(project, language).await?;
        return client
            .workspace_symbols_scoped(
                query,
                optional_string(args, "relative_path"),
                number(args, "limit", 100) as usize,
            )
            .await
            .map_err(backend_error);
    }
    let path = string(args, "relative_path")?;
    let client = engine.lsp_for(project, path).await?;
    match tool {
        "completion" => {
            let mut result = client
                .completion(path, position(args)?)
                .await
                .map_err(backend_error)?;
            let maximum = number(args, "limit", 50) as usize;
            let items = if result["result"].is_array() {
                result["result"].as_array_mut()
            } else {
                result["result"]["items"].as_array_mut()
            };
            if let Some(items) = items {
                let truncated = items.len() > maximum;
                items.truncate(maximum);
                result["truncated"] = json!(truncated);
            }
            result["commands_executed"] = json!(false);
            Ok(result)
        }
        "completion_resolve" => {
            let mut result = client
                .resolve_completion(path, &args["item"], string(args, "expected_sha256")?)
                .await
                .map_err(backend_error)?;
            result["commands_executed"] = json!(false);
            result["source_modified"] = json!(false);
            Ok(result)
        }
        "hover" => client
            .hover(path, position(args)?)
            .await
            .map_err(backend_error),
        "signature_help" => client
            .signature_help(path, position(args)?)
            .await
            .map_err(backend_error),
        "call_hierarchy" => client
            .call_hierarchy(path, position(args)?, string(args, "direction")?)
            .await
            .map_err(backend_error),
        "inlay_hints" => {
            let range = if args.get("range").is_some() {
                serde_json::from_value(args["range"].clone())?
            } else {
                let source = project.root.read(path)?;
                Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: end_position(&source),
                }
            };
            let mut result = client
                .inlay_hints(path, range)
                .await
                .map_err(backend_error)?;
            if let Some(items) = result["result"].as_array_mut() {
                let maximum = number(args, "limit", 200) as usize;
                let truncated = items.len() > maximum;
                items.truncate(maximum);
                result["truncated"] = json!(truncated);
            }
            result["commands_executed"] = json!(false);
            Ok(result)
        }
        "inlay_hint_resolve" => {
            let mut result = client
                .resolve_inlay_hint(path, &args["hint"], string(args, "expected_sha256")?)
                .await
                .map_err(backend_error)?;
            result["commands_executed"] = json!(false);
            result["source_modified"] = json!(false);
            Ok(result)
        }
        "semantic_tokens" => client
            .semantic_tokens(
                path,
                args.get("range")
                    .map(|value| serde_json::from_value(value.clone()))
                    .transpose()?,
            )
            .await
            .map_err(backend_error),
        "format_document" => {
            let before = read_expected(project, path, string(args, "expected_sha256")?)?;
            let options = args.get("options").unwrap_or(&Value::Null);
            let options = json!({"tabSize":number(options,"tab_size",4),"insertSpaces":boolean(options,"insert_spaces",true),
                "trimTrailingWhitespace":boolean(options,"trim_trailing_whitespace",false),"insertFinalNewline":boolean(options,"insert_final_newline",false),
                "trimFinalNewlines":boolean(options,"trim_final_newlines",false)});
            let edit = client
                .formatting(
                    path,
                    &options,
                    args.get("range")
                        .map(|value| serde_json::from_value(value.clone()))
                        .transpose()?,
                    string(args, "expected_sha256")?,
                )
                .await
                .map_err(backend_error)?;
            if project.root.read(path)? != before {
                return Err(Fault::conflict(
                    "Source changed while computing formatting edits",
                ));
            }
            if edit
                .get("changes")
                .and_then(Value::as_object)
                .is_some_and(|changes| {
                    changes.len() == 1
                        && changes
                            .values()
                            .all(|edits| edits.as_array().is_some_and(Vec::is_empty))
                })
                && edit.get("documentChanges").is_none()
                || edit
                    .get("documentChanges")
                    .and_then(Value::as_array)
                    .is_some_and(|changes| {
                        changes.len() == 1
                            && changes[0]["edits"].as_array().is_some_and(Vec::is_empty)
                    })
            {
                return Ok(
                    json!({"changed":false,"source_modified":false,"sha256":sha256(before.as_bytes()),"provider":"lsp","position_encoding":"utf-16"}),
                );
            }
            let snapshots = BTreeMap::from([(
                project.root.relative(&project.root.resolve(path, false)?)?,
                before,
            )]);
            let changes = match client.changes(&edit, &snapshots).await {
                Ok(changes) => changes,
                Err(error)
                    if error.to_string() == "WorkspaceEdit contains no effective changes" =>
                {
                    return Ok(
                        json!({"changed":false,"source_modified":false,"sha256":string(args,"expected_sha256")?,"provider":"lsp","position_encoding":"utf-16"}),
                    );
                }
                Err(error) => return Err(backend_error(error)),
            };
            let mut preview = project
                .transactions
                .prepare(changes, optional_string(args, "request_id"))?;
            preview["provider"] = json!("lsp");
            preview["operation"] = json!("formatting");
            preview["source_modified"] = json!(false);
            Ok(preview)
        }
        _ => Err(Fault::new("UNKNOWN_TOOL", "Unknown IDE operation")),
    }
}
fn position(args: &Value) -> Result<Position> {
    Ok(serde_json::from_value(args["position"].clone())?)
}
