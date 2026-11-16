//! Language-server IDE routes, inspections and refactor preparation.

use crate::{Engine, Project, ide, inspections, refactor};
use odin_core::{Fault, Result};
use serde_json::Value;
use std::sync::Arc;

impl Engine {
    pub(crate) async fn dispatch_semantic(
        self: &Arc<Self>,
        p: &Arc<Project>,
        tool: &str,
        a: &Value,
    ) -> Result<Value> {
        match tool {
            "completion" | "completion_resolve" | "hover" | "signature_help"
            | "workspace_symbols" | "call_hierarchy" | "format_document" | "inlay_hints"
            | "inlay_hint_resolve" | "semantic_tokens" => ide::dispatch(self, p, tool, a).await,
            "list_inspection_providers" | "run_inspections" | "prepare_inspection_fix" => {
                inspections::dispatch(self, p, tool, a).await
            }
            "prepare_code_action" | "move_symbol" | "inline_symbol" | "apply_quick_fix" => {
                refactor::prepare(self, p, tool, a).await
            }
            _ => Err(Fault::new(
                "UNKNOWN_TOOL",
                format!("No dispatcher for {tool}"),
            )),
        }
    }
}
