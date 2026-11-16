//! Policy-gated shell command execution and DAP debug sessions.

use crate::{
    DebugSession, Engine, Project, backend_error, boolean, optional_string, shell, string,
};
use odin_core::{Fault, Result};
use odin_protocol::dap::DebugAdapter;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

impl Engine {
    pub(crate) async fn dispatch_exec(
        self: &Arc<Self>,
        p: &Arc<Project>,
        tool: &str,
        a: &Value,
    ) -> Result<Value> {
        let limit = &self.config.limits;
        match tool {
            "execute_shell_command" => {
                let config = p
                    .config
                    .commands
                    .get(string(a, "profile")?)
                    .ok_or_else(|| Fault::denied("Unknown command profile"))?;
                if let Some(sandbox) = &p.config.sandbox {
                    shell::run_with_sandbox(
                        config,
                        p.root.root(),
                        limit.max_output_bytes,
                        Some((sandbox, p.state.as_path())),
                    )
                    .await
                } else {
                    shell::run(config, p.root.root(), limit.max_output_bytes).await
                }
            }
            "debug_start" => {
                if self.debuggers.lock().await.contains_key(&p.id) {
                    return Err(Fault::conflict("Debugger already running; stop it first"));
                }
                let profile = string(a, "profile")?;
                let config = p
                    .config
                    .debuggers
                    .get(profile)
                    .ok_or_else(|| Fault::denied("Unknown debugger profile"))?;
                let adapter = DebugAdapter::start_with_sandbox(
                    config,
                    &p.root,
                    p.config
                        .sandbox
                        .as_ref()
                        .map(|policy| (policy, p.state.as_path())),
                )
                .await
                .map_err(backend_error)?;
                let capabilities = adapter.capabilities.clone();
                let session_id = uuid::Uuid::new_v4().to_string();
                self.debuggers.lock().await.insert(
                    p.id.clone(),
                    DebugSession {
                        id: session_id.clone(),
                        profile: profile.into(),
                        adapter,
                    },
                );
                Ok(
                    json!({"started":true,"session_id":session_id,"capabilities":capabilities,"debuggee_launched":false,"unrestricted_host_debug":!p.config.debug_launches.contains_key(profile),"sandbox_requested":p.config.sandbox.is_some()}),
                )
            }
            "debug_request" | "debug_events" | "debug_stop" | "debug_batch" => {
                let (session_id, profile, adapter) = {
                    let debuggers = self.debuggers.lock().await;
                    let session = debuggers
                        .get(&p.id)
                        .ok_or_else(|| Fault::new("SESSION_CLOSED", "No active debug adapter"))?;
                    (
                        session.id.clone(),
                        session.profile.clone(),
                        session.adapter.clone(),
                    )
                };
                if optional_string(a, "session_id").is_some_and(|id| id != session_id) {
                    return Err(Fault::denied(
                        "Debug session does not belong to this active project session",
                    ));
                }
                if adapter.is_closed() && tool != "debug_stop" {
                    return Err(Fault::new("SESSION_CLOSED", "Debug adapter has exited"));
                }
                if tool == "debug_stop" {
                    let response = adapter
                        .request("disconnect", json!({"terminateDebuggee":true}))
                        .await;
                    let cleanup = adapter.shutdown().await;
                    self.debuggers.lock().await.remove(&p.id);
                    Ok(
                        json!({"adapter_dropped":true,"disconnect_succeeded":response.is_ok(),"cleanup_succeeded":cleanup.is_ok(),"warning":response.err().map(|e|e.to_string()),"cleanup_warning":cleanup.err().map(|e|e.to_string())}),
                    )
                } else if tool == "debug_events" {
                    let mut result = adapter.poll().await.map_err(backend_error)?;
                    result["unrestricted_host_debug"] =
                        json!(!p.config.debug_launches.contains_key(&profile));
                    Ok(result)
                } else if tool == "debug_batch" {
                    let commands = a["commands"]
                        .as_array()
                        .ok_or_else(|| Fault::invalid("commands must be an array"))?;
                    if commands.is_empty() || commands.len() > 32 {
                        return Err(Fault::invalid("Debug batches require 1..32 commands"));
                    }
                    let mut outcomes = Vec::new();
                    let mut successful = true;
                    let deadline =
                        tokio::time::Instant::now() + Duration::from_secs(limit.rpc_timeout_secs);
                    for command in commands {
                        let request_id = uuid::Uuid::new_v4().to_string();
                        let name = string(command, "command")?;
                        let arguments = match p.config.debug_arguments(
                            &profile,
                            name,
                            &command["arguments"],
                        ) {
                            Ok(arguments) => arguments,
                            Err(error) => {
                                successful = false;
                                outcomes.push(json!({"request_id":request_id,"command":name,"ok":false,"error":error}));
                                if boolean(a, "stop_on_error", true) {
                                    break;
                                } else {
                                    continue;
                                }
                            }
                        };
                        let result = match tokio::time::timeout_at(
                            deadline,
                            adapter.request(name, arguments),
                        )
                        .await
                        {
                            Ok(result) => result,
                            Err(_) => {
                                successful = false;
                                outcomes.push(json!({"request_id":request_id,"command":name,"ok":false,"error":{"code":"TIMEOUT","message":"Batch deadline elapsed; adapter-side outcome is unknown"}}));
                                break;
                            }
                        };
                        let ok = result.is_ok();
                        successful &= ok;
                        outcomes.push(match result{Ok(value)=>json!({"request_id":request_id,"command":name,"ok":true,"result":value}),
                            Err(error)=>json!({"request_id":request_id,"command":name,"ok":false,"error":error.to_string()})});
                        if !ok && boolean(a, "stop_on_error", true) {
                            break;
                        }
                    }
                    Ok(
                        json!({"session_id":session_id,"success":successful,"steps_executed":outcomes.len(),"outcomes":outcomes,"closed":adapter.is_closed(),"language":"odin-dap-batch","sandbox_requested":p.config.sandbox.is_some(),"unrestricted_host_debug":!p.config.debug_launches.contains_key(&profile)}),
                    )
                } else {
                    let command = string(a, "command")?;
                    if command == "cancel_ticket" {
                        adapter
                            .cancel(string(&a["arguments"], "ticket")?)
                            .await
                            .map_err(backend_error)
                    } else {
                        let arguments =
                            p.config
                                .debug_arguments(&profile, command, &a["arguments"])?;
                        adapter
                            .request(command, arguments)
                            .await
                            .map_err(backend_error)
                    }
                }
            }
            _ => Err(Fault::new(
                "UNKNOWN_TOOL",
                format!("No dispatcher for {tool}"),
            )),
        }
    }
}
