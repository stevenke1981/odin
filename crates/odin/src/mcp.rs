use odin_engine::Engine;
use rmcp::{
    ServerHandler,
    model::*,
    service::{RequestContext, RoleServer},
};
use serde_json::{Value, json};
use std::{borrow::Cow, collections::BTreeSet, sync::Arc};

/// Which catalog tiers are advertised over MCP. Every tool's schema costs agent
/// context on each session, so the default advertises only the everyday set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum ToolProfile {
    /// Everyday reading, search, symbols, references and transactional edits.
    #[default]
    Core,
    /// Core plus navigation, IDE queries and the remaining edit/refactor helpers.
    Ide,
    /// Every catalog tool with its unmodified contract (memories, flows, DAP...).
    Full,
}
impl ToolProfile {
    pub fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Ide => "ide",
            Self::Full => "full",
        }
    }
    fn of_tier(tier: &str) -> Self {
        match tier {
            "core" => Self::Core,
            "ide" => Self::Ide,
            _ => Self::Full,
        }
    }
}

/// The advertised MCP surface. Policy still lives in the engine: hiding a tool
/// is a context-size measure, never a permission grant or the only gate.
pub struct Exposure {
    profile: ToolProfile,
    tools: Vec<Value>,
    names: BTreeSet<String>,
    policy_hidden: BTreeSet<String>,
    hidden_by_policy: usize,
    total: usize,
}
impl Exposure {
    pub fn new(engine: &Engine, profile: ToolProfile, include: &[String]) -> anyhow::Result<Self> {
        let catalog = engine.tools();
        for name in include {
            if !catalog.iter().any(|tool| tool["name"] == name.as_str()) {
                anyhow::bail!("--include-tools names unknown tool {name}; see odin tools");
            }
        }
        let single = (engine.projects.len() == 1)
            .then(|| engine.projects.keys().next().cloned())
            .flatten();
        let mut tools = Vec::new();
        let mut policy_hidden = BTreeSet::new();
        for tool in &catalog {
            let name = tool["name"].as_str().unwrap_or("");
            if profile != ToolProfile::Full {
                let tier = ToolProfile::of_tier(tool["tier"].as_str().unwrap_or("full"));
                if tier > profile && !include.iter().any(|item| item == name) {
                    continue;
                }
                // Advertising a tool every project's policy rejects only costs context.
                if !engine
                    .projects
                    .values()
                    .any(|project| Engine::tool_enabled(&project.config, tool))
                {
                    policy_hidden.insert(name.to_owned());
                    continue;
                }
            }
            let mut tool = tool.clone();
            if let (Some(id), true) = (&single, profile != ToolProfile::Full) {
                // One configured project: the server already defaults to it.
                let schema = &mut tool["inputSchema"];
                if let Some(required) = schema["required"].as_array_mut() {
                    required.retain(|item| item != "project");
                    if required.is_empty() {
                        schema
                            .as_object_mut()
                            .map(|object| object.remove("required"));
                    }
                }
                schema["properties"]["project"]["description"] =
                    json!(format!("Optional; defaults to the only project, {id}"));
            }
            tools.push(tool);
        }
        let names = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
            .collect();
        Ok(Self {
            profile,
            tools,
            names,
            hidden_by_policy: policy_hidden.len(),
            policy_hidden,
            total: catalog.len(),
        })
    }
    fn summary(&self) -> String {
        let mut text = format!(
            "Tool profile {}: {} of {} tools advertised.",
            self.profile.name(),
            self.tools.len(),
            self.total
        );
        if self.hidden_by_policy > 0 {
            text.push_str(&format!(
                " {} more are hidden because host policy disables them (e.g. writes on a read-only project).",
                self.hidden_by_policy
            ));
        }
        if self.profile != ToolProfile::Full {
            text.push_str(" The operator can restart Odin with --tool-profile ide|full or --include-tools NAME for more.");
        }
        text
    }
}

#[derive(Clone)]
pub struct OdinMcp {
    pub engine: Arc<Engine>,
    pub exposure: Arc<Exposure>,
}
impl OdinMcp {
    fn sdk_tool(value: &Value) -> std::result::Result<Tool, serde_json::Error> {
        let mut value = value.clone();
        if let Some(object) = value.as_object_mut() {
            object.remove("policy");
            object.remove("tier");
        }
        serde_json::from_value(value)
    }
}
impl ServerHandler for OdinMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("odin", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!(
                "{}\nConfigured project IDs: {}{}\n{}",
                odin_engine::INSTRUCTIONS,
                self.engine
                    .projects
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
                if self.engine.projects.len() == 1 {
                    " (the project argument may be omitted)"
                } else {
                    ""
                },
                self.exposure.summary()
            ))
    }
    // Intentionally bounded compatibility contract. Newer SDK revisions are
    // not automatically advertised as application-level conformance evidence.
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Owned(
            ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"]
                .iter()
                .filter_map(|v| serde_json::from_value(json!(v)).ok())
                .collect(),
        )
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.exposure
            .tools
            .iter()
            .find(|tool| tool["name"] == name)
            .and_then(|t| Self::sdk_tool(t).ok())
    }
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self
            .exposure
            .tools
            .iter()
            .map(Self::sdk_tool)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.to_string();
        if !self.exposure.names.contains(&name) {
            let message = if self.exposure.policy_hidden.contains(&name) {
                format!(
                    "Tool {name} is disabled by host policy for every configured project (for edit tools: the project is read-only). Ask the operator to grant it in the host config (e.g. allow_write = true) and restart Odin"
                )
            } else if self.engine.tool(&name).is_some() {
                format!(
                    "Tool {name} is not in tool profile {}. Use an advertised tool, or ask the operator to restart Odin with --tool-profile full or --include-tools {name}",
                    self.exposure.profile.name()
                )
            } else {
                format!("Unknown tool: {name}")
            };
            return Err(ErrorData::new(ErrorCode::METHOD_NOT_FOUND, message, None));
        }
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let project = arguments["project"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| self.engine.default_project().ok());
        let result = tokio::select! {
            // If both the gate/backend and cancellation become ready together,
            // cancellation must win before polling the queued engine operation.
            biased;
            _ = context.ct.cancelled() => {
                return Err(ErrorData::new(
                    ErrorCode(-32800),
                    "Request cancelled; effects may already have occurred",
                    Some(json!({
                        "code":"REQUEST_CANCELLED",
                        "outcome":"UNKNOWN_IF_STARTED",
                        "guidance":"Inspect transaction journals and current hashes before retrying; cancellation does not roll back synchronous work already started."
                    })),
                ));
            }
            result = async {
                match project {
                    Some(project) => self.engine.call(project, name, arguments).await,
                    None => Err(odin_core::Fault::invalid("Provide an explicit project ID")),
                }
            } => result,
        };
        let (value, is_error) = match result {
            Ok(value) => (value, false),
            Err(mut error) => {
                // Errors may contain language-server or debugger output: bound it too.
                if error.message.len() > 8192 {
                    error.message =
                        error.message.chars().take(4096).collect::<String>() + " [truncated]";
                }
                (json!({"error":error}), true)
            }
        };
        let payload = json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":is_error});
        let result: CallToolResult = serde_json::from_value(payload)
            .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
        Ok(result.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use odin_core::config::Config;

    fn engine(writable: bool, projects: usize) -> (std::path::PathBuf, Arc<Engine>) {
        let base = std::env::temp_dir().join(format!(
            "odin-exposure-{}-{}-{writable}-{projects}",
            std::process::id(),
            odin_core::unix_ms()
        ));
        let mut text = format!("state_dir = {:?}\n", base.join("state"));
        for index in 0..projects {
            let root = base.join(format!("p{index}"));
            std::fs::create_dir_all(&root).unwrap();
            text.push_str(&format!(
                "[projects.p{index}]\nroot = {root:?}\nallow_write = {writable}\n"
            ));
        }
        std::fs::create_dir_all(&base).unwrap();
        let path = base.join("host.toml");
        std::fs::write(&path, text).unwrap();
        let config = Config::load(&path).unwrap();
        (base, Engine::new(config).unwrap())
    }

    #[test]
    fn core_profile_is_small_policy_filtered_and_project_optional() {
        let (base, engine) = engine(false, 1);
        let core = Exposure::new(&engine, ToolProfile::Core, &[]).unwrap();
        assert!(core.names.contains("find_symbol") && core.names.contains("read_file"));
        // Read-only project: write and shell tools are not advertised.
        assert!(!core.names.contains("transaction_apply"));
        assert!(!core.names.contains("execute_shell_command"));
        assert!(core.tools.len() < 20 && core.hidden_by_policy > 0);
        let read = core
            .tools
            .iter()
            .find(|t| t["name"] == "read_file")
            .unwrap();
        assert_eq!(read["inputSchema"]["required"], json!(["relative_path"]));
        assert!(core.summary().contains("--tool-profile"));
        let full = Exposure::new(&engine, ToolProfile::Full, &[]).unwrap();
        assert_eq!(full.tools, engine.tools());
        let ide = Exposure::new(&engine, ToolProfile::Ide, &["flow_run".into()]).unwrap();
        assert!(ide.names.contains("list_dir") && ide.names.contains("flow_run"));
        assert!(!ide.names.contains("debug_start"));
        assert!(Exposure::new(&engine, ToolProfile::Core, &["nope".into()]).is_err());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn writable_multi_project_keeps_edit_tools_and_required_project() {
        let (base, engine) = engine(true, 2);
        let core = Exposure::new(&engine, ToolProfile::Core, &[]).unwrap();
        assert!(core.names.contains("transaction_apply") && core.names.contains("rename_symbol"));
        let read = core
            .tools
            .iter()
            .find(|t| t["name"] == "read_file")
            .unwrap();
        assert!(
            read["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("project"))
        );
        let _ = std::fs::remove_dir_all(base);
    }
}
