//! Status, schema, configuration, onboarding and observability tools.

use crate::{
    Engine, INSTRUCTIONS, Project, backend_error, boolean, formatter, number_usize,
    optional_string, results,
};
use odin_core::{Fault, Result, search, sha256};
use serde_json::{Value, json};
use std::sync::Arc;

impl Engine {
    pub(crate) async fn dispatch_meta(
        self: &Arc<Self>,
        p: &Arc<Project>,
        tool: &str,
        a: &Value,
    ) -> Result<Value> {
        let limit = &self.config.limits;
        match tool {
            "status" => Ok(
                json!({"name":"Odin","version":env!("CARGO_PKG_VERSION"),"project":p.id,
                "native_language":"Rust","standalone":true,"external_tool_bridge":false,"tool_count":self.catalog.len(),"native_or_protocol_tools":self.catalog.len(),
                "capabilities_are_backend_dependent":true,"native_read_only":!p.config.allow_write,
                "transaction_visibility":"per-file atomic, journaled multi-file recovery","sandbox":false}),
            ),
            "get_version" => Ok(
                json!({"name":"Odin","version":env!("CARGO_PKG_VERSION"),"schema_version":1,"standalone":true}),
            ),
            "get_schemas" => {
                if let Some(name) = optional_string(a, "tool") {
                    let tool = self.tool(name).ok_or_else(|| {
                        Fault::new("UNKNOWN_TOOL", format!("Unknown tool: {name}"))
                    })?;
                    Ok(json!({"schema_version":1,"tool":tool}))
                } else {
                    Ok(json!({"schema_version":1,"tools":self.catalog}))
                }
            }
            "get_capabilities" => {
                let requested = optional_string(a, "language");
                if requested
                    .is_some_and(|id| !p.config.languages.iter().any(|language| language.id == id))
                {
                    return Err(Fault::new(
                        "MISSING_BACKEND",
                        "Requested language is not configured by the host",
                    ));
                }
                let mut languages = Vec::new();
                for language in p
                    .config
                    .languages
                    .iter()
                    .filter(|language| requested.is_none_or(|id| language.id == id))
                {
                    let existing = self
                        .lsp
                        .lock()
                        .await
                        .get(&(p.id.clone(), language.id.clone()))
                        .cloned();
                    let client = if boolean(a, "probe", false) {
                        Some(self.lsp_language(p, language.clone()).await)
                    } else {
                        existing.map(Ok)
                    };
                    let mut entry = json!({"id":language.id,"extensions":language.extensions,"configured":true,"probed":client.is_some(),"ready":false});
                    match client {
                        Some(Ok(client)) => {
                            entry["ready"] = json!(!client.is_closed());
                            entry["capabilities"] = client.capabilities.clone();
                            entry["status"] = client.status().map_err(backend_error)?;
                        }
                        Some(Err(error)) => entry["error"] = serde_json::to_value(error)?,
                        None => {}
                    }
                    languages.push(entry);
                }
                let tools = self
                    .catalog
                    .iter()
                    .map(|tool| {
                        let name = tool["name"].as_str().unwrap_or("");
                        let enabled = !p.config.excluded_tools.iter().any(|item| item == name)
                            && (!matches!(name, "run_inspections" | "prepare_inspection_fix")
                                || (p.config.allow_inspections && p.config.sandbox.is_some()))
                            && match tool["policy"].as_str() {
                                Some("write") => p.config.allow_write,
                                Some("execute") if name.starts_with("debug_") => {
                                    p.config.allow_debug
                                }
                                Some("execute") if name == "run_inspections" => {
                                    p.config.allow_inspections
                                }
                                Some("execute") => p.config.allow_shell,
                                _ => true,
                            };
                        json!({"name":name,"policy":tool["policy"],"enabled":enabled})
                    })
                    .collect::<Vec<_>>();
                Ok(
                    json!({"project":p.id,"standalone":true,"external_tool_bridge":false,"semantic_available":languages.iter().any(|l|l["ready"]==true),"languages":languages,
                    "tools":tools,"write_allowed":p.config.allow_write,"shell_allowed":p.config.allow_shell,"debug_allowed":p.config.allow_debug,
                    "inspections_allowed":p.config.allow_inspections,"inspection_profiles":p.config.inspections.iter().map(|(name,profile)|json!({"name":name,"provider":profile.provider,"requires_sandbox":true,"enabled":p.config.allow_inspections&&p.config.sandbox.is_some()})).collect::<Vec<_>>(),
                    "formatters_allowed":p.config.allow_formatters,"formatter_profiles":formatter::profiles(p),
                    "result_retention":{"ttl_ms":results::TTL_MS,"max_results":results::MAX_RESULTS,"max_bytes":results::MAX_STORAGE_BYTES}}),
                )
            }
            "list_projects" => Ok(self.project_summaries()),
            "get_current_config" => Ok(
                json!({"project":p.id,"root":p.root.root(),"allow_write":p.config.allow_write,
                "allow_shell":p.config.allow_shell,"allow_debug":p.config.allow_debug,"allow_inspections":p.config.allow_inspections,"allow_formatters":p.config.allow_formatters,"formatter_profiles":formatter::profiles(p),"limits":limit,
                "excluded_tools":p.config.excluded_tools,"deny_globs":p.config.deny_globs,
                "languages":p.config.languages.iter().map(|l|json!({"id":l.id,"extensions":l.extensions})).collect::<Vec<_>>(),
                "command_profiles":p.config.commands.keys().collect::<Vec<_>>(),"debug_profiles":p.config.debuggers.keys().collect::<Vec<_>>(),"inspection_profiles":p.config.inspections.iter().map(|(name,profile)|json!({"name":name,"provider":profile.provider})).collect::<Vec<_>>()}),
            ),
            "initial_instructions" => Ok(json!({"instructions":INSTRUCTIONS})),
            "onboarding" => {
                let files = search::files(&p.root, limit.max_files)?;
                let recognized = [
                    "Cargo.toml",
                    "package.json",
                    "pyproject.toml",
                    "AGENTS.md",
                    "README.md",
                    "CMakeLists.txt",
                    "go.mod",
                ];
                let evidence=recognized.iter().filter_map(|path|p.root.read(path).ok().map(|body|json!({"path":path,"sha256":sha256(body.as_bytes()),"content":body}))).collect::<Vec<_>>();
                Ok(
                    json!({"files":files,"evidence":evidence,"memories":self.memories.list(&p.id)?,
                    "guidance":"Read evidence; record architecture, conventions and verified build commands in separate Markdown memories. Do not claim any test ran during onboarding."}),
                )
            }
            "result_read" => results::read(self, p, a),
            "audit_log" => p.store.recent(number_usize(a, "limit", 50)),
            _ => Err(Fault::new(
                "UNKNOWN_TOOL",
                format!("No dispatcher for {tool}"),
            )),
        }
    }
}
