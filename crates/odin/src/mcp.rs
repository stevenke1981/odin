use odin_engine::Engine;
use rmcp::{
    ServerHandler,
    model::*,
    service::{RequestContext, RoleServer},
};
use serde_json::{Value, json};
use std::{borrow::Cow, sync::Arc};

#[derive(Clone)]
pub struct OdinMcp {
    pub engine: Arc<Engine>,
}
impl OdinMcp {
    fn sdk_tool(value: &Value) -> std::result::Result<Tool, serde_json::Error> {
        let mut value = value.clone();
        if let Some(object) = value.as_object_mut() {
            object.remove("policy");
        }
        serde_json::from_value(value)
    }
}
impl ServerHandler for OdinMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("odin", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!(
                "{}\nConfigured project IDs: {}",
                odin_engine::INSTRUCTIONS,
                self.engine
                    .projects
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
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
        self.engine.tool(name).and_then(|t| Self::sdk_tool(t).ok())
    }
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self
            .engine
            .tools()
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
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.to_string();
        if self.engine.tool(&name).is_none() {
            return Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Unknown tool: {name}"),
                None,
            ));
        }
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let project = arguments["project"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| self.engine.default_project().ok());
        let result = match project {
            Some(project) => self.engine.call(project, name, arguments).await,
            None => Err(odin_core::Fault::invalid("Provide an explicit project ID")),
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
