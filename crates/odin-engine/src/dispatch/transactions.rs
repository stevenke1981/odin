//! Native transaction prepare, preview, apply, rollback and recovery.

use crate::{Engine, Project, optional_string, string};
use odin_core::{Fault, Result};
use serde_json::Value;
use std::sync::Arc;

impl Engine {
    pub(crate) async fn dispatch_transactions(
        self: &Arc<Self>,
        p: &Arc<Project>,
        tool: &str,
        a: &Value,
    ) -> Result<Value> {
        match tool {
            "transaction_prepare" => p.transactions.prepare(
                serde_json::from_value(a["changes"].clone())?,
                optional_string(a, "request_id"),
            ),
            "transaction_preview" => p.transactions.preview(string(a, "transaction_id")?),
            "transaction_list" => p.transactions.list(),
            "transaction_apply" | "transaction_rollback" => {
                let mut result = if tool == "transaction_apply" {
                    p.transactions.apply(string(a, "transaction_id")?)?
                } else {
                    p.transactions.rollback(string(a, "transaction_id")?)?
                };
                self.refresh_backends(p, &mut result).await?;
                Ok(result)
            }
            "transaction_recover" => {
                let result = p.transactions.recover()?;
                self.stop_language_servers(&p.id).await;
                Ok(result)
            }
            _ => Err(Fault::new(
                "UNKNOWN_TOOL",
                format!("No dispatcher for {tool}"),
            )),
        }
    }
}
