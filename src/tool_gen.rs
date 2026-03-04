use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::{JsonObject, Tool};
use serde_json::json;

use crate::config::{ArgConfig, CliConfig, JsonType, ToolConfig};

/// A resolved command ready for execution.
/// Maps an MCP tool name back to the CLI invocation details.
#[derive(Debug, Clone)]
pub struct ResolvedCommand {
    pub executable: String,
    pub command_parts: Vec<String>,
    pub args: Vec<ArgConfig>,
    pub output_schema: Option<JsonObject>,
    pub raw_args: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: Option<std::path::PathBuf>,
}

/// Result of generating tools from config.
#[derive(Debug)]
pub struct GeneratedTools {
    pub tools: Vec<Tool>,
    pub commands: HashMap<String, ResolvedCommand>,
}

/// Generate MCP tools and a command lookup map from the config.
pub fn generate_tools(config: &CliConfig) -> Result<GeneratedTools, String> {
    let mut tools = Vec::new();
    let mut commands = HashMap::new();

    for tool_config in &config.tools {
        let tool_name = tool_config.tool_name(&config.cli.name);

        if commands.contains_key(&tool_name) {
            return Err(format!("Duplicate tool name: '{}'", tool_name));
        }

        // Parse output_schema once and share between MCP tool and resolved command
        let parsed_output_schema = parse_output_schema(&tool_name, tool_config)?;

        let mcp_tool = build_mcp_tool(&tool_name, tool_config, &parsed_output_schema)?;
        let resolved = build_resolved_command(config, tool_config, parsed_output_schema);

        tools.push(mcp_tool);
        commands.insert(tool_name, resolved);
    }

    Ok(GeneratedTools { tools, commands })
}

/// Parse the output_schema string from config into a JsonObject, if present.
fn parse_output_schema(
    tool_name: &str,
    tool_config: &ToolConfig,
) -> Result<Option<JsonObject>, String> {
    match &tool_config.output_schema {
        Some(schema_str) => {
            let schema_value: serde_json::Value =
                serde_json::from_str(schema_str).map_err(|e| {
                    format!("Invalid output_schema JSON for tool '{}': {}", tool_name, e)
                })?;
            let schema_obj = schema_value.as_object().ok_or_else(|| {
                format!(
                    "output_schema for tool '{}' must be a JSON object",
                    tool_name
                )
            })?;
            Ok(Some(schema_obj.clone()))
        }
        None => Ok(None),
    }
}

fn build_mcp_tool(
    tool_name: &str,
    tool_config: &ToolConfig,
    parsed_output_schema: &Option<JsonObject>,
) -> Result<Tool, String> {
    let input_schema = build_input_schema(&tool_config.args);
    let input_schema_arc: Arc<JsonObject> = Arc::new(input_schema);

    let mut tool = Tool::new(
        tool_name.to_string(),
        tool_config.description.clone(),
        input_schema_arc,
    );

    if let Some(ref schema_obj) = parsed_output_schema {
        tool.output_schema = Some(Arc::new(schema_obj.clone()));
    }

    Ok(tool)
}

fn build_input_schema(args: &[ArgConfig]) -> JsonObject {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for arg in args {
        let prop = build_arg_property(arg);
        // Use underscored name for JSON Schema property keys (dashes aren't ideal in JSON)
        let prop_name = arg.name.replace('-', "_");
        properties.insert(prop_name.clone(), prop);

        if arg.required {
            required.push(serde_json::Value::String(prop_name));
        }
    }

    let mut schema = serde_json::Map::new();
    schema.insert("type".to_string(), json!("object"));
    schema.insert(
        "properties".to_string(),
        serde_json::Value::Object(properties),
    );
    if !required.is_empty() {
        schema.insert("required".to_string(), serde_json::Value::Array(required));
    }
    schema
}

fn build_arg_property(arg: &ArgConfig) -> serde_json::Value {
    match arg.json_type {
        JsonType::Array => {
            json!({
                "type": "array",
                "description": arg.description,
                "items": { "type": "string" }
            })
        }
        _ => {
            json!({
                "type": arg.json_type.as_schema_type(),
                "description": arg.description,
            })
        }
    }
}

fn build_resolved_command(
    config: &CliConfig,
    tool_config: &ToolConfig,
    parsed_output_schema: Option<JsonObject>,
) -> ResolvedCommand {
    // Merge env: cli-level as base, tool-level overrides
    let mut env = config.cli.env.clone();
    env.extend(tool_config.env.clone());

    // Tool-level working_dir takes precedence over cli-level
    let working_dir = tool_config
        .working_dir
        .clone()
        .or_else(|| config.cli.working_dir.clone());

    ResolvedCommand {
        executable: config.cli.executable.clone(),
        command_parts: tool_config.command.clone(),
        args: tool_config.args.clone(),
        output_schema: parsed_output_schema,
        raw_args: tool_config.raw_args.clone(),
        env,
        working_dir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ArgType, ArrayStyle, CliMeta};

    fn minimal_config(tools: Vec<ToolConfig>) -> CliConfig {
        CliConfig {
            cli: CliMeta {
                name: "npm".to_string(),
                executable: "npm".to_string(),
                env: HashMap::new(),
                working_dir: None,
            },
            tools,
        }
    }

    fn make_tool(command: Vec<&str>, description: &str, args: Vec<ArgConfig>) -> ToolConfig {
        ToolConfig {
            command: command.into_iter().map(String::from).collect(),
            description: description.to_string(),
            args,
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        }
    }

    #[test]
    fn test_single_tool_name() {
        let config = minimal_config(vec![make_tool(vec!["install"], "Install packages", vec![])]);
        let result = generate_tools(&config).unwrap();

        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.tools[0].name.as_ref(), "npm_install");
        assert_eq!(
            result.tools[0].description.as_deref(),
            Some("Install packages")
        );
    }

    #[test]
    fn test_multi_part_command_name() {
        let config = minimal_config(vec![
            make_tool(vec!["config", "set"], "Set config", vec![]),
            make_tool(vec!["config", "get"], "Get config", vec![]),
        ]);
        let result = generate_tools(&config).unwrap();

        assert_eq!(result.tools.len(), 2);
        assert_eq!(result.tools[0].name.as_ref(), "npm_config_set");
        assert_eq!(result.tools[1].name.as_ref(), "npm_config_get");
    }

    #[test]
    fn test_input_schema_with_args() {
        let args = vec![
            ArgConfig {
                name: "package".to_string(),
                arg_type: ArgType::Positional,
                description: "Package name".to_string(),
                required: true,
                json_type: JsonType::String,
                array_style: ArrayStyle::default(),
            },
            ArgConfig {
                name: "save-dev".to_string(),
                arg_type: ArgType::Flag,
                description: "Save as dev dep".to_string(),
                required: false,
                json_type: JsonType::Boolean,
                array_style: ArrayStyle::default(),
            },
        ];
        let config = minimal_config(vec![make_tool(vec!["install"], "Install", args)]);
        let result = generate_tools(&config).unwrap();

        let schema = result.tools[0].schema_as_json_value();
        let props = schema["properties"].as_object().unwrap();

        // Check property types
        assert_eq!(props["package"]["type"], "string");
        assert_eq!(props["package"]["description"], "Package name");
        assert_eq!(props["save_dev"]["type"], "boolean");
        assert_eq!(props["save_dev"]["description"], "Save as dev dep");

        // Check required
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "package");
    }

    #[test]
    fn test_array_arg_schema() {
        let args = vec![ArgConfig {
            name: "packages".to_string(),
            arg_type: ArgType::Positional,
            description: "Packages to install".to_string(),
            required: true,
            json_type: JsonType::Array,
            array_style: ArrayStyle::Space,
        }];
        let config = minimal_config(vec![make_tool(vec!["install"], "Install", args)]);
        let result = generate_tools(&config).unwrap();

        let schema = result.tools[0].schema_as_json_value();
        let props = schema["properties"].as_object().unwrap();
        assert_eq!(props["packages"]["type"], "array");
        assert_eq!(props["packages"]["items"]["type"], "string");
    }

    #[test]
    fn test_output_schema_attached() {
        let mut tool_config = make_tool(vec!["view"], "View package", vec![]);
        tool_config.output_schema =
            Some(r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#.to_string());

        let config = minimal_config(vec![tool_config]);
        let result = generate_tools(&config).unwrap();

        assert!(result.tools[0].output_schema.is_some());
        let output_schema = result.tools[0].output_schema.as_ref().unwrap();
        assert_eq!(output_schema["type"], "object");
    }

    #[test]
    fn test_invalid_output_schema_errors() {
        let mut tool_config = make_tool(vec!["view"], "View", vec![]);
        tool_config.output_schema = Some("not valid json".to_string());

        let config = minimal_config(vec![tool_config]);
        let result = generate_tools(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid output_schema JSON"));
    }

    #[test]
    fn test_command_lookup_map() {
        let args = vec![ArgConfig {
            name: "key".to_string(),
            arg_type: ArgType::Positional,
            description: "Config key".to_string(),
            required: true,
            json_type: JsonType::String,
            array_style: ArrayStyle::default(),
        }];
        let config = minimal_config(vec![make_tool(vec!["config", "get"], "Get config", args)]);
        let result = generate_tools(&config).unwrap();

        let cmd = result.commands.get("npm_config_get").unwrap();
        assert_eq!(cmd.executable, "npm");
        assert_eq!(cmd.command_parts, vec!["config", "get"]);
        assert_eq!(cmd.args.len(), 1);
        assert_eq!(cmd.args[0].name, "key");
    }

    #[test]
    fn test_multiple_tools_all_generated() {
        let config = minimal_config(vec![
            make_tool(vec!["install"], "Install", vec![]),
            make_tool(vec!["run"], "Run script", vec![]),
            make_tool(vec!["config", "set"], "Set config", vec![]),
        ]);
        let result = generate_tools(&config).unwrap();

        assert_eq!(result.tools.len(), 3);
        assert_eq!(result.commands.len(), 3);
        assert!(result.commands.contains_key("npm_install"));
        assert!(result.commands.contains_key("npm_run"));
        assert!(result.commands.contains_key("npm_config_set"));
    }

    #[test]
    fn test_env_merging() {
        let mut config = minimal_config(vec![make_tool(vec!["install"], "Install", vec![])]);
        config
            .cli
            .env
            .insert("NODE_ENV".to_string(), "production".to_string());
        config
            .cli
            .env
            .insert("DEBUG".to_string(), "false".to_string());
        config.tools[0]
            .env
            .insert("DEBUG".to_string(), "true".to_string());

        let result = generate_tools(&config).unwrap();
        let cmd = result.commands.get("npm_install").unwrap();

        // Tool-level overrides cli-level
        assert_eq!(cmd.env.get("DEBUG").unwrap(), "true");
        // CLI-level preserved when not overridden
        assert_eq!(cmd.env.get("NODE_ENV").unwrap(), "production");
    }

    #[test]
    fn test_working_dir_precedence() {
        let mut config = minimal_config(vec![make_tool(vec!["install"], "Install", vec![])]);
        config.cli.working_dir = Some("/cli/dir".into());
        config.tools[0].working_dir = Some("/tool/dir".into());

        let result = generate_tools(&config).unwrap();
        let cmd = result.commands.get("npm_install").unwrap();
        assert_eq!(cmd.working_dir, Some(std::path::PathBuf::from("/tool/dir")));

        // Without tool-level override, uses cli-level
        let mut config2 = minimal_config(vec![make_tool(vec!["run"], "Run", vec![])]);
        config2.cli.working_dir = Some("/cli/dir".into());
        let result2 = generate_tools(&config2).unwrap();
        let cmd2 = result2.commands.get("npm_run").unwrap();
        assert_eq!(cmd2.working_dir, Some(std::path::PathBuf::from("/cli/dir")));
    }

    #[test]
    fn test_no_required_omits_field() {
        let args = vec![ArgConfig {
            name: "verbose".to_string(),
            arg_type: ArgType::Flag,
            description: "Verbose".to_string(),
            required: false,
            json_type: JsonType::Boolean,
            array_style: ArrayStyle::default(),
        }];
        let config = minimal_config(vec![make_tool(vec!["install"], "Install", args)]);
        let result = generate_tools(&config).unwrap();

        let schema = result.tools[0].schema_as_json_value();
        // When no args are required, the "required" field should not be present
        assert!(schema.get("required").is_none());
    }

    #[test]
    fn test_duplicate_tool_names_errors() {
        let config = minimal_config(vec![
            make_tool(vec!["install"], "Install packages", vec![]),
            make_tool(vec!["install"], "Install again", vec![]),
        ]);
        let result = generate_tools(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate tool name"));
    }

    #[test]
    fn test_non_object_output_schema_errors() {
        let mut tool_config = make_tool(vec!["view"], "View", vec![]);
        tool_config.output_schema = Some(r#"[1, 2, 3]"#.to_string());

        let config = minimal_config(vec![tool_config]);
        let result = generate_tools(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be a JSON object"));
    }

    #[test]
    fn test_empty_tools_list() {
        let config = minimal_config(vec![]);
        let result = generate_tools(&config).unwrap();
        assert!(result.tools.is_empty());
        assert!(result.commands.is_empty());
    }

    #[test]
    fn test_all_json_types() {
        let args = vec![
            ArgConfig {
                name: "name".to_string(),
                arg_type: ArgType::Option,
                description: "Name".to_string(),
                required: false,
                json_type: JsonType::String,
                array_style: ArrayStyle::default(),
            },
            ArgConfig {
                name: "count".to_string(),
                arg_type: ArgType::Option,
                description: "Count".to_string(),
                required: false,
                json_type: JsonType::Integer,
                array_style: ArrayStyle::default(),
            },
            ArgConfig {
                name: "ratio".to_string(),
                arg_type: ArgType::Option,
                description: "Ratio".to_string(),
                required: false,
                json_type: JsonType::Number,
                array_style: ArrayStyle::default(),
            },
            ArgConfig {
                name: "verbose".to_string(),
                arg_type: ArgType::Flag,
                description: "Verbose".to_string(),
                required: false,
                json_type: JsonType::Boolean,
                array_style: ArrayStyle::default(),
            },
        ];
        let config = minimal_config(vec![make_tool(vec!["test"], "Test", args)]);
        let result = generate_tools(&config).unwrap();

        let schema = result.tools[0].schema_as_json_value();
        let props = schema["properties"].as_object().unwrap();
        assert_eq!(props["name"]["type"], "string");
        assert_eq!(props["count"]["type"], "integer");
        assert_eq!(props["ratio"]["type"], "number");
        assert_eq!(props["verbose"]["type"], "boolean");
    }
}
