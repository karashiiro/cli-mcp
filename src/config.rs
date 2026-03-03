use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level configuration for the CLI-to-MCP bridge.
#[derive(Debug, Clone, Deserialize)]
pub struct CliConfig {
    pub cli: CliMeta,
    pub tools: Vec<ToolConfig>,
}

/// Metadata about the CLI executable being wrapped.
#[derive(Debug, Clone, Deserialize)]
pub struct CliMeta {
    pub name: String,
    pub executable: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
}

/// A single tool definition mapping to one CLI command invocation.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolConfig {
    pub command: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub args: Vec<ArgConfig>,
    pub output_schema: Option<String>,
    #[serde(default)]
    pub raw_args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
}

impl ToolConfig {
    /// Generate the MCP tool name from the CLI name and command parts.
    /// e.g., cli name "npm" + command ["config", "set"] → "npm_config_set"
    pub fn tool_name(&self, cli_name: &str) -> String {
        let mut parts = vec![cli_name.to_string()];
        parts.extend(self.command.iter().cloned());
        parts.join("_")
    }
}

/// Configuration for a single argument to a CLI command.
#[derive(Debug, Clone, Deserialize)]
pub struct ArgConfig {
    pub name: String,
    pub arg_type: ArgType,
    pub description: String,
    #[serde(default)]
    pub required: bool,
    pub json_type: JsonType,
    #[serde(default)]
    pub array_style: ArrayStyle,
}

/// How the argument is passed on the command line.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ArgType {
    Positional,
    Flag,
    Option,
}

/// The JSON Schema type for the argument value.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JsonType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
}

impl JsonType {
    /// Convert to the JSON Schema type string.
    pub fn as_schema_type(&self) -> &'static str {
        match self {
            JsonType::String => "string",
            JsonType::Number => "number",
            JsonType::Integer => "integer",
            JsonType::Boolean => "boolean",
            JsonType::Array => "array",
        }
    }
}

/// How array-type arguments are passed on the command line.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ArrayStyle {
    /// --tag a --tag b
    #[default]
    Repeated,
    /// --tag a,b
    Comma,
    /// a b (space-separated, typically for positional)
    Space,
}

/// Parse a TOML string into a CliConfig.
pub fn parse_config(toml_str: &str) -> Result<CliConfig, toml::de::Error> {
    toml::from_str(toml_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
[cli]
name = "npm"
executable = "npm"

[[tools]]
command = ["install"]
description = "Install packages"
"#;
        let config = parse_config(toml).unwrap();
        assert_eq!(config.cli.name, "npm");
        assert_eq!(config.cli.executable, "npm");
        assert!(config.cli.env.is_empty());
        assert!(config.cli.working_dir.is_none());
        assert_eq!(config.tools.len(), 1);
        assert_eq!(config.tools[0].command, vec!["install"]);
        assert_eq!(config.tools[0].description, "Install packages");
        assert!(config.tools[0].args.is_empty());
    }

    #[test]
    fn test_parse_multi_part_commands() {
        let toml = r#"
[cli]
name = "npm"
executable = "npm"

[[tools]]
command = ["config", "set"]
description = "Set a config key"

[[tools]]
command = ["config", "get"]
description = "Get a config value"
"#;
        let config = parse_config(toml).unwrap();
        assert_eq!(config.tools.len(), 2);
        assert_eq!(config.tools[0].command, vec!["config", "set"]);
        assert_eq!(config.tools[1].command, vec!["config", "get"]);
    }

    #[test]
    fn test_parse_output_schema() {
        let toml = r#"
[cli]
name = "npm"
executable = "npm"

[[tools]]
command = ["view"]
description = "View package info"
output_schema = '{"type": "object", "properties": {"name": {"type": "string"}}}'
raw_args = ["--json"]
"#;
        let config = parse_config(toml).unwrap();
        let tool = &config.tools[0];
        assert!(tool.output_schema.is_some());
        let schema_str = tool.output_schema.as_ref().unwrap();
        assert!(schema_str.contains("\"type\": \"object\""));
        assert_eq!(tool.raw_args, vec!["--json"]);
    }

    #[test]
    fn test_parse_all_arg_types() {
        let toml = r#"
[cli]
name = "npm"
executable = "npm"

[[tools]]
command = ["install"]
description = "Install packages"

[[tools.args]]
name = "package"
arg_type = "positional"
description = "Package name"
required = false
json_type = "string"

[[tools.args]]
name = "save-dev"
arg_type = "flag"
description = "Save as dev dependency"
required = false
json_type = "boolean"

[[tools.args]]
name = "registry"
arg_type = "option"
description = "Registry URL"
required = false
json_type = "string"
"#;
        let config = parse_config(toml).unwrap();
        let args = &config.tools[0].args;
        assert_eq!(args.len(), 3);

        assert_eq!(args[0].name, "package");
        assert_eq!(args[0].arg_type, ArgType::Positional);
        assert_eq!(args[0].json_type, JsonType::String);
        assert!(!args[0].required);

        assert_eq!(args[1].name, "save-dev");
        assert_eq!(args[1].arg_type, ArgType::Flag);
        assert_eq!(args[1].json_type, JsonType::Boolean);

        assert_eq!(args[2].name, "registry");
        assert_eq!(args[2].arg_type, ArgType::Option);
        assert_eq!(args[2].json_type, JsonType::String);
    }

    #[test]
    fn test_parse_array_args_with_style() {
        let toml = r#"
[cli]
name = "npm"
executable = "npm"

[[tools]]
command = ["install"]
description = "Install packages"

[[tools.args]]
name = "packages"
arg_type = "positional"
description = "Packages"
required = true
json_type = "array"
array_style = "space"

[[tools.args]]
name = "tag"
arg_type = "option"
description = "Tags"
required = false
json_type = "array"
array_style = "repeated"
"#;
        let config = parse_config(toml).unwrap();
        let args = &config.tools[0].args;
        assert_eq!(args[0].json_type, JsonType::Array);
        assert_eq!(args[0].array_style, ArrayStyle::Space);
        assert_eq!(args[1].json_type, JsonType::Array);
        assert_eq!(args[1].array_style, ArrayStyle::Repeated);
    }

    #[test]
    fn test_array_style_defaults_to_repeated() {
        let toml = r#"
[cli]
name = "npm"
executable = "npm"

[[tools]]
command = ["install"]
description = "Install"

[[tools.args]]
name = "packages"
arg_type = "positional"
description = "Packages"
required = true
json_type = "array"
"#;
        let config = parse_config(toml).unwrap();
        assert_eq!(config.tools[0].args[0].array_style, ArrayStyle::Repeated);
    }

    #[test]
    fn test_reject_missing_required_fields() {
        // Missing cli section
        let toml = r#"
[[tools]]
command = ["install"]
description = "Install"
"#;
        assert!(parse_config(toml).is_err());

        // Missing description
        let toml = r#"
[cli]
name = "npm"
executable = "npm"

[[tools]]
command = ["install"]
"#;
        assert!(parse_config(toml).is_err());
    }

    #[test]
    fn test_tool_name_generation() {
        let tool = ToolConfig {
            command: vec!["config".to_string(), "set".to_string()],
            description: "Set config".to_string(),
            args: vec![],
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        assert_eq!(tool.tool_name("npm"), "npm_config_set");

        let tool2 = ToolConfig {
            command: vec!["install".to_string()],
            description: "Install".to_string(),
            args: vec![],
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        assert_eq!(tool2.tool_name("npm"), "npm_install");
    }

    #[test]
    fn test_parse_env_and_working_dir() {
        let toml = r#"
[cli]
name = "npm"
executable = "npm"
working_dir = "/tmp/project"

[cli.env]
NODE_ENV = "production"

[[tools]]
command = ["install"]
description = "Install"
working_dir = "/tmp/other"

[tools.env]
DEBUG = "true"
"#;
        let config = parse_config(toml).unwrap();
        assert_eq!(config.cli.working_dir, Some(PathBuf::from("/tmp/project")));
        assert_eq!(config.cli.env.get("NODE_ENV").unwrap(), "production");
        assert_eq!(
            config.tools[0].working_dir,
            Some(PathBuf::from("/tmp/other"))
        );
        assert_eq!(config.tools[0].env.get("DEBUG").unwrap(), "true");
    }

    #[test]
    fn test_json_type_as_schema_type() {
        assert_eq!(JsonType::String.as_schema_type(), "string");
        assert_eq!(JsonType::Number.as_schema_type(), "number");
        assert_eq!(JsonType::Integer.as_schema_type(), "integer");
        assert_eq!(JsonType::Boolean.as_schema_type(), "boolean");
        assert_eq!(JsonType::Array.as_schema_type(), "array");
    }
}
