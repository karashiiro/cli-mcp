use std::collections::HashMap;
use std::future::Future;

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool, ToolsCapability,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ErrorData as McpError;

use crate::executor::execute_command;
use crate::tool_gen::ResolvedCommand;

/// The MCP server that wraps a CLI tool.
pub struct CliMcpServer {
    tools: Vec<Tool>,
    commands: HashMap<String, ResolvedCommand>,
    server_name: String,
}

impl CliMcpServer {
    pub fn new(
        server_name: String,
        tools: Vec<Tool>,
        commands: HashMap<String, ResolvedCommand>,
    ) -> Self {
        Self {
            tools,
            commands,
            server_name,
        }
    }

    /// Handle a tool call by name. Shared logic for both ServerHandler and direct testing.
    pub async fn handle_call(
        &self,
        tool_name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> CallToolResult {
        let resolved = match self.commands.get(tool_name) {
            Some(cmd) => cmd,
            None => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Unknown tool: {}",
                    tool_name
                ))]);
            }
        };

        execute_command(resolved, &arguments).await
    }
}

impl ServerHandler for CliMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: self.server_name.clone(),
                title: None,
                version: env!("CARGO_PKG_VERSION").to_string(),
                description: None,
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: None }),
                ..Default::default()
            },
            instructions: Some(format!(
                "This server wraps the '{}' CLI tool. Use the available tools to execute CLI commands.",
                self.server_name
            )),
            ..Default::default()
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: self.tools.clone(),
            next_cursor: None,
            meta: None,
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let tool_name = request.name.to_string();
        let arguments = request.arguments.unwrap_or_default();

        async move { Ok(self.handle_call(&tool_name, arguments).await) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ArgConfig, ArgType, ArrayStyle, CliConfig, CliMeta, JsonType, ToolConfig};
    use crate::tool_gen::generate_tools;

    fn test_config() -> CliConfig {
        CliConfig {
            cli: CliMeta {
                name: "echo".to_string(),
                executable: "echo".to_string(),
                env: HashMap::new(),
                working_dir: None,
            },
            tools: vec![
                ToolConfig {
                    command: vec!["hello".to_string()],
                    description: "Say hello".to_string(),
                    args: vec![ArgConfig {
                        name: "name".to_string(),
                        arg_type: ArgType::Positional,
                        description: "Name".to_string(),
                        required: true,
                        json_type: JsonType::String,
                        array_style: ArrayStyle::default(),
                    }],
                    output_schema: None,
                    raw_args: vec![],
                    env: HashMap::new(),
                    working_dir: None,
                },
                ToolConfig {
                    command: vec!["goodbye".to_string()],
                    description: "Say goodbye".to_string(),
                    args: vec![],
                    output_schema: None,
                    raw_args: vec![],
                    env: HashMap::new(),
                    working_dir: None,
                },
            ],
        }
    }

    fn make_server(config: &CliConfig) -> CliMcpServer {
        let generated = generate_tools(config).unwrap();
        CliMcpServer::new(
            format!("cli-mcp-{}", config.cli.name),
            generated.tools,
            generated.commands,
        )
    }

    #[test]
    fn test_get_info() {
        let config = test_config();
        let server = make_server(&config);
        let info = server.get_info();

        assert_eq!(info.server_info.name, "cli-mcp-echo");
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
        assert!(info.capabilities.tools.is_some());
    }

    #[test]
    fn test_tools_list() {
        let config = test_config();
        let server = make_server(&config);

        assert_eq!(server.tools.len(), 2);
        let names: Vec<&str> = server.tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(names.contains(&"echo_hello"));
        assert!(names.contains(&"echo_goodbye"));
    }

    #[tokio::test]
    async fn test_call_tool_success() {
        let config = test_config();
        let server = make_server(&config);

        let mut args = serde_json::Map::new();
        args.insert(
            "name".to_string(),
            serde_json::Value::String("world".to_string()),
        );

        let result = server.handle_call("echo_hello", args).await;
        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().unwrap().text.trim();
        assert_eq!(text, "hello world");
    }

    #[tokio::test]
    async fn test_call_unknown_tool() {
        let config = test_config();
        let server = make_server(&config);

        let result = server
            .handle_call("nonexistent_tool", serde_json::Map::new())
            .await;
        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_call_tool_no_args() {
        let config = test_config();
        let server = make_server(&config);

        let result = server
            .handle_call("echo_goodbye", serde_json::Map::new())
            .await;
        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().unwrap().text.trim();
        assert_eq!(text, "goodbye");
    }
}
