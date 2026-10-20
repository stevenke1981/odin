use crate::{Engine, Project, backend_error, optional_string, read_expected, string};
use odin_core::{Fault, Result, search, sha256};
use odin_protocol::text::Range;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

fn select(actions: &Value, title: &str, requested_kind: Option<&str>) -> Result<Value> {
    let items = actions
        .as_array()
        .ok_or_else(|| Fault::new("CAPABILITY_UNAVAILABLE", "Backend returned no code actions"))?;
    let matching = items
        .iter()
        .filter(|action| {
            action["title"] == title
                && requested_kind.is_none_or(|kind| {
                    action["kind"].as_str().is_some_and(|value| {
                        value == kind || value.starts_with(&format!("{kind}."))
                    })
                })
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(Fault::new(
            "CAPABILITY_UNAVAILABLE",
            format!(
                "Expected one backend code action with this title and kind; found {}",
                matching.len()
            ),
        ));
    }
    let action = matching[0];
    if action.get("disabled").is_some_and(|value| !value.is_null()) {
        return Err(Fault::new(
            "CAPABILITY_UNAVAILABLE",
            "Backend code action is disabled",
        ));
    }
    reject_command(action)?;
    Ok(action.clone())
}

fn reject_command(action: &Value) -> Result<()> {
    if action.get("command").is_some_and(|value| !value.is_null()) {
        return Err(Fault::denied(
            "Code actions with server commands cannot be applied; Odin accepts edit-only actions",
        ));
    }
    Ok(())
}

pub async fn prepare(
    engine: &Arc<Engine>,
    project: &Arc<Project>,
    tool: &str,
    args: &Value,
) -> Result<Value> {
    let path = string(args, "relative_path")?;
    let before = read_expected(project, path, string(args, "expected_sha256")?)?;
    let client = engine.lsp_for(project, path).await?;
    let range = if tool == "move_symbol" || tool == "inline_symbol" {
        client
            .symbol(path, string(args, "name_path")?)
            .await
            .map_err(backend_error)?
            .range
    } else {
        serde_json::from_value::<Range>(args["range"].clone())?
    };
    let kind = match tool {
        "move_symbol" => Some("refactor.move"),
        "inline_symbol" => Some("refactor.inline"),
        "apply_quick_fix" => Some("quickfix"),
        _ => optional_string(args, "action_kind"),
    };
    let listing = search::files(&project.root, engine.config.limits.max_files)?;
    if listing.truncated {
        return Err(Fault::new(
            "LIMIT_EXCEEDED",
            "Code action snapshot would omit files",
        ));
    }
    let mut snapshots = BTreeMap::new();
    let mut bytes = 0usize;
    for file in listing.files {
        if let Ok(text) = project.root.read(&file) {
            bytes += text.len();
            if bytes > 64 * 1024 * 1024 {
                return Err(Fault::new(
                    "LIMIT_EXCEEDED",
                    "Code action snapshot exceeds 64 MiB",
                ));
            }
            snapshots.insert(file, text);
        }
    }
    if snapshots.get(path) != Some(&before) {
        return Err(Fault::conflict(
            "Target changed while resolving code action range",
        ));
    }
    let actions = if tool == "apply_quick_fix" {
        let report = client.diagnostics(path).await.map_err(backend_error)?;
        if report["available"] != true {
            return Err(Fault::new(
                "CAPABILITY_UNAVAILABLE",
                "Quick fix requires an available diagnostics report",
            ));
        }
        let diagnostics = report["report"]
            .get("items")
            .or_else(|| report["report"].get("diagnostics"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter(|diagnostic| {
                        serde_json::from_value::<Range>(diagnostic["range"].clone()).is_ok_and(
                            |diagnostic| {
                                diagnostic.start <= range.end && range.start <= diagnostic.end
                            },
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        client
            .actions_with_diagnostics(path, range, &diagnostics)
            .await
            .map_err(backend_error)?
    } else {
        client.actions(path, range).await.map_err(backend_error)?
    };
    let mut action = select(&actions, string(args, "action_title")?, kind)?;
    if action.get("edit").is_none_or(Value::is_null) {
        action = client
            .resolve_action(&action)
            .await
            .map_err(backend_error)?;
        action = select(&json!([action]), string(args, "action_title")?, kind)?;
    }
    reject_command(&action)?;
    let workspace_edit = action
        .get("edit")
        .filter(|edit| edit.is_object())
        .ok_or_else(|| {
            Fault::new(
                "CAPABILITY_UNAVAILABLE",
                "Backend action has no WorkspaceEdit; no textual fallback exists",
            )
        })?;
    let changes = client
        .changes(workspace_edit, &snapshots)
        .await
        .map_err(backend_error)?;
    if tool == "move_symbol" {
        let target = string(args, "target_path")?;
        let target_absolute = project.root.resolve(target, true)?;
        let target = target_absolute
            .strip_prefix(project.root.root())
            .map_err(|_| Fault::denied("Move destination is outside project"))?
            .to_str()
            .ok_or_else(|| Fault::invalid("Move destination must be UTF-8"))?
            .replace('\\', "/");
        let source = project.root.relative(&project.root.resolve(path, false)?)?;
        if source == target {
            return Err(Fault::invalid("Move target must differ from source"));
        }
        if !changes.iter().any(|change| change.path == target)
            || !changes.iter().any(|change| change.path == source)
        {
            return Err(Fault::new(
                "CAPABILITY_UNAVAILABLE",
                "Backend Move action does not edit both the requested source and destination",
            ));
        }
    }
    let mut preview = project
        .transactions
        .prepare(changes, optional_string(args, "request_id"))?;
    preview["code_action"] = json!({"title":action["title"],"kind":action["kind"],"backend":"lsp","server_commands_executed":false,
        "workspace_edit_sha256":sha256(&serde_json::to_vec(workspace_edit)?)});
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn does_not_accept_command_only_or_mixed_action() {
        for action in [
            json!({"title":"Fix","command":"danger"}),
            json!({"title":"Fix","edit":{"changes":{}},"command":{"command":"danger"}}),
        ] {
            assert_eq!(
                select(&json!([action]), "Fix", None).unwrap_err().code,
                "PERMISSION_DENIED"
            );
        }
    }
    #[test]
    fn enforces_semantic_action_kind_and_unambiguous_title() {
        assert!(
            select(
                &json!([{"title":"Inline","kind":"quickfix","edit":{}}]),
                "Inline",
                Some("refactor.inline")
            )
            .is_err()
        );
        assert!(
            select(
                &json!([{"title":"Inline","kind":"refactor.inline","edit":{}}]),
                "Inline",
                Some("refactor.inline")
            )
            .is_ok()
        );
        assert!(select(&json!([{"title":"Fix"},{"title":"Fix"}]), "Fix", None).is_err());
        assert!(
            select(
                &json!([{"title":"Fix","disabled":{"reason":"Unavailable"}}]),
                "Fix",
                None
            )
            .is_err()
        );
    }
}
