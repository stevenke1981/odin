//! Language-server symbol queries, diagnostics and code actions.

use crate::{Engine, Project, backend_error, boolean, string, symbols};
use odin_core::{Fault, Result};
use odin_protocol::text::{Position, Range};
use serde_json::{Value, json};
use std::sync::Arc;

impl Engine {
    pub(crate) async fn dispatch_symbols(
        self: &Arc<Self>,
        p: &Arc<Project>,
        tool: &str,
        a: &Value,
    ) -> Result<Value> {
        let limit = &self.config.limits;
        match tool {
            "get_symbols_overview" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let mut options = a.clone();
                options["query"] = json!("");
                let filter = symbols::Filter::new(&options, limit.max_matches)?;
                let found = client
                    .symbols(path)
                    .await
                    .map_err(backend_error)?
                    .into_iter()
                    .filter(|symbol| filter.accepts(symbol))
                    .collect::<Vec<_>>();
                let has_next = found.len() > filter.offset.saturating_add(filter.limit);
                let page = found
                    .into_iter()
                    .skip(filter.offset)
                    .take(filter.limit)
                    .collect::<Vec<_>>();
                let mut result = json!({"symbols":page,"backend":"lsp","language":client.language.id,"offset":filter.offset,"truncated":has_next,
                    "next_offset":if has_next{Some(filter.offset+filter.limit)}else{None}});
                if boolean(a, "group_by_file", false) {
                    result["groups"] = json!({path:result["symbols"]});
                }
                Ok(result)
            }
            "find_symbol" => self.find_symbols(p, a).await,
            "find_referencing_symbols" | "find_implementations" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let symbol = client
                    .symbol(path, string(a, "name_path")?)
                    .await
                    .map_err(backend_error)?;
                let kind = if tool == "find_referencing_symbols" {
                    "references"
                } else {
                    "implementation"
                };
                let locations = client
                    .at(path, symbol.selection_range.start, kind)
                    .await
                    .map_err(backend_error)?;
                self.enrich_locations(p, locations).await
            }
            "find_declaration" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let position: Position = serde_json::from_value(a["position"].clone())?;
                let kind = if boolean(a, "declaration_only", false) {
                    "declaration"
                } else {
                    "definition"
                };
                self.enrich_locations(
                    p,
                    client
                        .at(path, position, kind)
                        .await
                        .map_err(backend_error)?,
                )
                .await
            }
            "get_type_hierarchy" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let symbol = client
                    .symbol(path, string(a, "name_path")?)
                    .await
                    .map_err(backend_error)?;
                client
                    .hierarchy(path, symbol.selection_range.start, string(a, "direction")?)
                    .await
                    .map_err(backend_error)
            }
            "get_diagnostics_for_file" | "get_diagnostics_for_symbol" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let mut report = client.diagnostics(path).await.map_err(backend_error)?;
                if tool == "get_diagnostics_for_symbol" && report["available"] == true {
                    let symbol = client
                        .symbol(path, string(a, "name_path")?)
                        .await
                        .map_err(backend_error)?;
                    for key in ["items", "diagnostics"] {
                        if let Some(items) = report["report"][key].as_array_mut() {
                            items.retain(|item| {
                                serde_json::from_value::<Range>(item["range"].clone()).is_ok_and(
                                    |r| r.start < symbol.range.end && symbol.range.start <= r.end,
                                )
                            });
                        }
                    }
                    if let Some(findings) = report["findings"].as_array_mut() {
                        findings.retain(|finding| {
                            serde_json::from_value::<Range>(finding["range"].clone()).is_ok_and(
                                |range| {
                                    range.start < symbol.range.end
                                        && symbol.range.start <= range.end
                                },
                            )
                        });
                        let counts = (1..=4)
                            .map(|severity| {
                                findings
                                    .iter()
                                    .filter(|finding| finding["lsp_severity"] == severity)
                                    .count()
                            })
                            .collect::<Vec<_>>();
                        let total = findings.len();
                        let unknown = total - counts.iter().sum::<usize>();
                        let status = if report["freshness"] != "current" {
                            report["status"].clone()
                        } else if counts[0] > 0 {
                            json!("errors")
                        } else if counts[1] > 0 {
                            json!("warnings")
                        } else if total > 0 {
                            json!("findings")
                        } else {
                            json!("clean")
                        };
                        for (key, count) in ["errors", "warnings", "information", "hints"]
                            .into_iter()
                            .zip(counts)
                        {
                            report["summary"][key] = json!(count);
                        }
                        report["summary"]["total"] = json!(total);
                        report["summary"]["unknown"] = json!(unknown);
                        report["status"] = status;
                    }
                    report["summary"]["scope"] = json!("symbol");
                    report["scope"] = json!({"relative_path":path,"name_path":string(a,"name_path")?,"range":symbol.range});
                    report["name_path"] = symbol.name_path.into();
                }
                Ok(report)
            }
            "code_actions" => {
                let path = string(a, "relative_path")?;
                let client = self.lsp_for(p, path).await?;
                let range: Range = serde_json::from_value(a["range"].clone())?;
                Ok(
                    json!({"actions":client.actions(path,range).await.map_err(backend_error)?,"executed":false}),
                )
            }
            "restart_language_server" => {
                let warnings = self.stop_language_servers(&p.id).await;
                Ok(
                    json!({"stopped":true,"shutdown_warnings":warnings,"restart":"lazy on next semantic request"}),
                )
            }
            _ => Err(Fault::new(
                "UNKNOWN_TOOL",
                format!("No dispatcher for {tool}"),
            )),
        }
    }
}
