use crate::rpc::{Dialect, Framing, RpcProcess};
use anyhow::{Result, anyhow, bail};
use odin_core::{config::CommandConfig, paths::SafeRoot};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex, oneshot},
    task::JoinHandle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Initialized,
    Launching,
    Configured,
    Running,
    Stopped,
    Terminated,
    Disconnected,
    OutcomeUnknown,
}

struct State {
    phase: Phase,
    initialized_event: bool,
    generation: u64,
    configured: bool,
    events: VecDeque<Value>,
}

pub struct DebugAdapter {
    rpc: Arc<RpcProcess>,
    pub capabilities: Value,
    launches: Mutex<BTreeMap<String, JoinHandle<Result<Value>>>>,
    request_gate: Mutex<()>,
    state: Arc<StdMutex<State>>,
    next_ticket: AtomicU64,
}

impl DebugAdapter {
    pub async fn start(config: &CommandConfig, root: &SafeRoot) -> Result<Arc<Self>> {
        let rpc = RpcProcess::spawn(
            &config.program,
            &config.args,
            root.root(),
            Framing::ContentLength,
            Dialect::Dap,
            Duration::from_secs(config.timeout_secs),
            json!([]),
            Value::Null,
        )?;
        let capabilities = rpc.request("initialize", json!({
            "clientID":"odin","clientName":"Odin","adapterID":"configured-adapter",
            "locale":"en-US","linesStartAt1":true,"columnsStartAt1":true,"pathFormat":"path",
            "supportsVariableType":true,"supportsRunInTerminalRequest":false,"supportsProgressReporting":false,
            "supportsMemoryReferences":false,"supportsInvalidatedEvent":true,
        })).await?;
        if !capabilities.is_object() {
            bail!("Invalid DAP initialize capabilities");
        }
        rpc.set_cancel_supported(capabilities["supportsCancelRequest"] == true);
        Ok(Arc::new(Self {
            rpc,
            capabilities,
            launches: Mutex::new(BTreeMap::new()),
            request_gate: Mutex::new(()),
            state: Arc::new(StdMutex::new(State {
                phase: Phase::Initialized,
                initialized_event: false,
                generation: 0,
                configured: false,
                events: VecDeque::new(),
            })),
            next_ticket: AtomicU64::new(1),
        }))
    }

    fn capture_events(&self) -> Result<()> {
        let events = self.rpc.events()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("DAP state lock poisoned"))?;
        for event in events {
            match event["event"].as_str() {
                Some("initialized") => {
                    state.initialized_event = true;
                    state.configured = false;
                }
                Some("stopped") => state.phase = Phase::Stopped,
                Some("continued") => state.phase = Phase::Running,
                Some("terminated") => state.phase = Phase::Terminated,
                _ => {}
            }
            if state.events.len() >= 256 {
                state.events.pop_front();
            }
            state.events.push_back(event);
        }
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.rpc.is_closed()
            || self
                .state
                .lock()
                .map(|s| s.phase == Phase::Disconnected)
                .unwrap_or(true)
    }

    pub fn status(&self) -> Result<Value> {
        self.capture_events()?;
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow!("DAP state lock poisoned"))?;
        Ok(
            json!({"phase":state.phase,"initialized_event":state.initialized_event,"configured":state.configured,
            "generation":state.generation,"closed":self.rpc.is_closed() || state.phase == Phase::Disconnected,
            "capabilities":self.capabilities,"transport":self.rpc.status()?}),
        )
    }

    fn require_capability(&self, name: &str) -> Result<()> {
        if self.capabilities[name] != true {
            bail!("CAPABILITY_UNAVAILABLE: {name}");
        }
        Ok(())
    }

    fn validate(&self, command: &str) -> Result<()> {
        let capability = match command {
            "configurationDone" => Some("supportsConfigurationDoneRequest"),
            "setFunctionBreakpoints" => Some("supportsFunctionBreakpoints"),
            "terminate" => Some("supportsTerminateRequest"),
            "restart" => Some("supportsRestartRequest"),
            "loadedSources" => Some("supportsLoadedSourcesRequest"),
            "exceptionInfo" => Some("supportsExceptionInfoRequest"),
            "setVariable" => Some("supportsSetVariable"),
            _ => None,
        };
        if let Some(capability) = capability {
            self.require_capability(capability)?;
        }
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow!("DAP state lock poisoned"))?;
        let phase = state.phase;
        let active = matches!(
            phase,
            Phase::Launching | Phase::Configured | Phase::Running | Phase::Stopped
        );
        let allowed = match command {
            "launch" | "attach" => phase == Phase::Initialized,
            "configurationDone" => active && state.initialized_event && !state.configured,
            "setBreakpoints" | "setFunctionBreakpoints" | "setExceptionBreakpoints" => {
                state.initialized_event && active
            }
            "stackTrace" | "scopes" | "variables" | "evaluate" | "setVariable"
            | "exceptionInfo" | "continue" | "next" | "stepIn" | "stepOut" => {
                phase == Phase::Stopped
            }
            "pause" => phase == Phase::Running,
            "threads" | "source" | "loadedSources" => active,
            "terminate" => active,
            "restart" => active || matches!(phase, Phase::Terminated | Phase::OutcomeUnknown),
            "disconnect" => phase != Phase::Disconnected,
            _ => false,
        };
        if !allowed {
            bail!("INVALID_DAP_STATE: {command} is not allowed in {phase:?}");
        }
        Ok(())
    }

    pub async fn request(&self, command: &str, args: Value) -> Result<Value> {
        let _gate = self.request_gate.lock().await;
        const ALLOWED: &[&str] = &[
            "launch",
            "attach",
            "setBreakpoints",
            "setFunctionBreakpoints",
            "setExceptionBreakpoints",
            "configurationDone",
            "threads",
            "stackTrace",
            "scopes",
            "variables",
            "evaluate",
            "continue",
            "next",
            "stepIn",
            "stepOut",
            "pause",
            "disconnect",
            "terminate",
            "restart",
            "source",
            "loadedSources",
            "exceptionInfo",
            "setVariable",
        ];
        if !ALLOWED.contains(&command) {
            bail!("Unsupported DAP command; no raw command execution fallback");
        }
        if !args.is_object() {
            bail!("DAP arguments must be an object");
        }
        self.capture_events()?;
        if self.is_closed() {
            bail!("Debug adapter is closed; explicitly start a new session");
        }
        self.validate(command)?;
        if matches!(command, "launch" | "attach" | "restart") {
            let mut jobs = self.launches.lock().await;
            if jobs.len() >= 8 {
                bail!("Drain launch tickets before starting another launch");
            }
            self.validate(command)?;
            let ticket = format!(
                "launch-{}",
                self.next_ticket.fetch_add(1, Ordering::Relaxed)
            );
            let generation = {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| anyhow!("DAP state lock poisoned"))?;
                state.generation += 1;
                state.phase = Phase::Launching;
                state.generation
            };
            let rpc = self.rpc.clone();
            let state = self.state.clone();
            let command = command.to_string();
            let (started_tx, started_rx) = oneshot::channel();
            let job = tokio::spawn(async move {
                let result = rpc.request_signaled(&command, args, Some(started_tx)).await;
                if let Ok(mut state) = state.lock()
                    && state.generation == generation
                {
                    if result.is_err()
                        && matches!(state.phase, Phase::Launching | Phase::Configured)
                    {
                        state.phase = Phase::OutcomeUnknown;
                    } else if result.is_ok()
                        && matches!(state.phase, Phase::Launching | Phase::Configured)
                    {
                        state.phase = Phase::Running;
                    }
                }
                result
            });
            if started_rx.await.is_err() {
                return match job.await {
                    Ok(Err(error)) => Err(error),
                    Err(error) => Err(error.into()),
                    _ => Err(anyhow!("Launch frame was not sent")),
                };
            }
            jobs.insert(ticket.clone(), job);
            return Ok(
                json!({"ticket":ticket,"pending":true,"next":"Poll events, configure breakpoints, then send configurationDone if required"}),
            );
        }
        let resumes = matches!(command, "continue" | "next" | "stepIn" | "stepOut");
        if resumes {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("DAP state lock poisoned"))?;
            state.phase = Phase::Running;
        }
        let outcome = self.rpc.request(command, args).await;
        self.capture_events()?;
        let result = match outcome {
            Ok(value) => value,
            Err(error) => {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| anyhow!("DAP state lock poisoned"))?;
                if resumes && state.phase == Phase::Running {
                    state.phase = Phase::OutcomeUnknown;
                }
                return Err(error);
            }
        };
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("DAP state lock poisoned"))?;
            match command {
                "configurationDone" => {
                    state.configured = true;
                    if state.phase == Phase::Launching {
                        state.phase = Phase::Configured;
                    }
                }
                "disconnect" => state.phase = Phase::Disconnected,
                _ => {}
            }
        }
        if command == "disconnect" {
            self.rpc.terminate().await?;
        }
        Ok(result)
    }

    /// Cancellation is best-effort and cannot prove a debuggee was never started.
    pub async fn cancel(&self, ticket: &str) -> Result<Value> {
        let job = self
            .launches
            .lock()
            .await
            .remove(ticket)
            .ok_or_else(|| anyhow!("Unknown launch ticket"))?;
        job.abort();
        let _ = job.await;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("DAP state lock poisoned"))?;
        if matches!(state.phase, Phase::Launching | Phase::Configured) {
            state.phase = Phase::OutcomeUnknown;
        }
        Ok(json!({"ticket":ticket,"cancelled":true,"outcome_may_be_unknown":true}))
    }

    pub async fn shutdown(&self) -> Result<()> {
        if self.is_closed() {
            return self.rpc.terminate().await;
        }
        let outcome = self.request("disconnect", json!({})).await;
        let cleanup = self.rpc.terminate().await;
        outcome?;
        cleanup
    }

    pub async fn poll(&self) -> Result<Value> {
        self.capture_events()?;
        let mut jobs = self.launches.lock().await;
        let completed = jobs
            .iter()
            .filter(|(_, job)| job.is_finished())
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let mut results = Vec::new();
        for id in completed {
            let job = jobs
                .remove(&id)
                .ok_or_else(|| anyhow!("Missing launch ticket"))?;
            let result = match job.await {
                Ok(Ok(value)) => json!({"ok":true,"result":value}),
                Ok(Err(error)) => json!({"ok":false,"error":error.to_string()}),
                Err(error) => json!({"ok":false,"error":error.to_string()}),
            };
            results.push(json!({"ticket":id,"outcome":result}));
        }
        self.capture_events()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("DAP state lock poisoned"))?;
        Ok(
            json!({"events":state.events.drain(..).collect::<Vec<_>>(),"launch_results":results,
            "pending_tickets":jobs.keys().collect::<Vec<_>>(),"phase":state.phase,"generation":state.generation}),
        )
    }
}

impl Drop for DebugAdapter {
    fn drop(&mut self) {
        for (_, job) in std::mem::take(self.launches.get_mut()) {
            job.abort();
        }
    }
}
