use std::collections::HashMap;

use rmcp::model::{CallToolResult, Content};
use serde_json::Value;
use tokio::process::Command;

use crate::config::{ArgType, ArrayStyle, JsonType};
use crate::tool_gen::ResolvedCommand;

/// Execute a resolved command with the given arguments from the MCP tool call.
pub async fn execute_command(
    resolved: &ResolvedCommand,
    arguments: &serde_json::Map<String, Value>,
) -> CallToolResult {
    let mut cmd = Command::new(&resolved.executable);

    // Add command parts (e.g., ["config", "set"])
    for part in &resolved.command_parts {
        cmd.arg(part);
    }

    // Build args from the tool call arguments, respecting order and types
    if let Err(e) = apply_arguments(&mut cmd, resolved, arguments) {
        return CallToolResult::error(vec![Content::text(format!(
            "Failed to build command arguments: {}",
            e
        ))]);
    }

    // Append raw_args
    for raw_arg in &resolved.raw_args {
        cmd.arg(raw_arg);
    }

    // Set environment variables
    for (key, value) in &resolved.env {
        cmd.env(key, value);
    }

    // Set working directory
    if let Some(ref dir) = resolved.working_dir {
        cmd.current_dir(dir);
    }

    // Execute
    let output = match cmd.output().await {
        Ok(output) => output,
        Err(e) => {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to execute command: {}",
                e
            ))]);
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        format_success_result(stdout, stderr, resolved)
    } else {
        format_error_result(stdout, stderr)
    }
}

/// Apply arguments to the command based on arg configs.
/// Processes positional args first (in config order), then flags and options.
fn apply_arguments(
    cmd: &mut Command,
    resolved: &ResolvedCommand,
    arguments: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    // First pass: positional args in config order
    for arg_config in &resolved.args {
        if arg_config.arg_type != ArgType::Positional {
            continue;
        }
        let prop_name = arg_config.name.replace('-', "_");
        if let Some(value) = arguments.get(&prop_name) {
            apply_positional(cmd, value, &arg_config.array_style)?;
        }
    }

    // Second pass: flags and options
    for arg_config in &resolved.args {
        let prop_name = arg_config.name.replace('-', "_");
        let flag_name = format!("--{}", arg_config.name);

        match arg_config.arg_type {
            ArgType::Flag => {
                if let Some(Value::Bool(true)) = arguments.get(&prop_name) {
                    cmd.arg(&flag_name);
                }
            }
            ArgType::Option => {
                if let Some(value) = arguments.get(&prop_name) {
                    apply_option(cmd, &flag_name, value, &arg_config.json_type, &arg_config.array_style)?;
                }
            }
            ArgType::Positional => {} // Already handled
        }
    }

    Ok(())
}

fn apply_positional(
    cmd: &mut Command,
    value: &Value,
    array_style: &ArrayStyle,
) -> Result<(), String> {
    match value {
        Value::Array(items) => match array_style {
            ArrayStyle::Space | ArrayStyle::Repeated => {
                for item in items {
                    cmd.arg(value_to_string(item));
                }
            }
            ArrayStyle::Comma => {
                let joined: Vec<String> = items.iter().map(value_to_string).collect();
                cmd.arg(joined.join(","));
            }
        },
        other => {
            cmd.arg(value_to_string(other));
        }
    }
    Ok(())
}

fn apply_option(
    cmd: &mut Command,
    flag_name: &str,
    value: &Value,
    _json_type: &JsonType,
    array_style: &ArrayStyle,
) -> Result<(), String> {
    match value {
        Value::Array(items) => match array_style {
            ArrayStyle::Repeated => {
                for item in items {
                    cmd.arg(flag_name);
                    cmd.arg(value_to_string(item));
                }
            }
            ArrayStyle::Comma => {
                let joined: Vec<String> = items.iter().map(value_to_string).collect();
                cmd.arg(flag_name);
                cmd.arg(joined.join(","));
            }
            ArrayStyle::Space => {
                cmd.arg(flag_name);
                for item in items {
                    cmd.arg(value_to_string(item));
                }
            }
        },
        other => {
            cmd.arg(flag_name);
            cmd.arg(value_to_string(other));
        }
    }
    Ok(())
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

fn format_success_result(
    stdout: String,
    _stderr: String,
    resolved: &ResolvedCommand,
) -> CallToolResult {
    if resolved.output_schema.is_some() {
        // Must parse stdout as JSON and return structured content
        match serde_json::from_str::<Value>(&stdout) {
            Ok(json_value) => CallToolResult::structured(json_value),
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "Command succeeded but output is not valid JSON (outputSchema declared): {}\nOutput: {}",
                e, stdout
            ))]),
        }
    } else {
        CallToolResult::success(vec![Content::text(stdout)])
    }
}

fn format_error_result(stdout: String, stderr: String) -> CallToolResult {
    let mut message = String::new();
    if !stdout.is_empty() {
        message.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !message.is_empty() {
            message.push('\n');
        }
        message.push_str(&stderr);
    }
    if message.is_empty() {
        message = "Command failed with no output".to_string();
    }
    CallToolResult::error(vec![Content::text(message)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ArgConfig, ArgType, ArrayStyle, JsonType};
    use crate::tool_gen::ResolvedCommand;
    use std::path::PathBuf;

    fn echo_command(args: Vec<ArgConfig>) -> ResolvedCommand {
        ResolvedCommand {
            executable: "echo".to_string(),
            command_parts: vec![],
            args,
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        }
    }

    fn make_args(pairs: &[(&str, Value)]) -> serde_json::Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_simple_command_success() {
        let resolved = echo_command(vec![]);
        let args = serde_json::Map::new();

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(false));
        assert!(!result.content.is_empty());
    }

    #[tokio::test]
    async fn test_positional_args_ordering() {
        let args_config = vec![
            ArgConfig {
                name: "first".to_string(),
                arg_type: ArgType::Positional,
                description: "First arg".to_string(),
                required: true,
                json_type: JsonType::String,
                array_style: ArrayStyle::default(),
            },
            ArgConfig {
                name: "second".to_string(),
                arg_type: ArgType::Positional,
                description: "Second arg".to_string(),
                required: true,
                json_type: JsonType::String,
                array_style: ArrayStyle::default(),
            },
        ];
        let resolved = echo_command(args_config);
        let args = make_args(&[
            ("first", Value::String("hello".to_string())),
            ("second", Value::String("world".to_string())),
        ]);

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().unwrap().text.trim();
        assert_eq!(text, "hello world");
    }

    #[tokio::test]
    async fn test_flag_args() {
        // Use printf to avoid echo -n portability issues
        let resolved = ResolvedCommand {
            executable: "printf".to_string(),
            command_parts: vec!["%s ".to_string()],
            args: vec![
                ArgConfig {
                    name: "verbose".to_string(),
                    arg_type: ArgType::Flag,
                    description: "Verbose".to_string(),
                    required: false,
                    json_type: JsonType::Boolean,
                    array_style: ArrayStyle::default(),
                },
            ],
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        let args = make_args(&[("verbose", Value::Bool(true))]);

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("--verbose"));
    }

    #[tokio::test]
    async fn test_option_args() {
        let resolved = ResolvedCommand {
            executable: "echo".to_string(),
            command_parts: vec![],
            args: vec![ArgConfig {
                name: "registry".to_string(),
                arg_type: ArgType::Option,
                description: "Registry".to_string(),
                required: false,
                json_type: JsonType::String,
                array_style: ArrayStyle::default(),
            }],
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        let args = make_args(&[(
            "registry",
            Value::String("https://registry.npmjs.org".to_string()),
        )]);

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().unwrap().text.trim();
        assert_eq!(text, "--registry https://registry.npmjs.org");
    }

    #[tokio::test]
    async fn test_array_args_repeated() {
        let resolved = ResolvedCommand {
            executable: "echo".to_string(),
            command_parts: vec![],
            args: vec![ArgConfig {
                name: "tag".to_string(),
                arg_type: ArgType::Option,
                description: "Tags".to_string(),
                required: false,
                json_type: JsonType::Array,
                array_style: ArrayStyle::Repeated,
            }],
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        let args = make_args(&[(
            "tag",
            Value::Array(vec![
                Value::String("foo".to_string()),
                Value::String("bar".to_string()),
            ]),
        )]);

        let result = execute_command(&resolved, &args).await;
        let text = result.content[0].as_text().unwrap().text.trim();
        assert_eq!(text, "--tag foo --tag bar");
    }

    #[tokio::test]
    async fn test_array_args_comma() {
        let resolved = ResolvedCommand {
            executable: "echo".to_string(),
            command_parts: vec![],
            args: vec![ArgConfig {
                name: "tag".to_string(),
                arg_type: ArgType::Option,
                description: "Tags".to_string(),
                required: false,
                json_type: JsonType::Array,
                array_style: ArrayStyle::Comma,
            }],
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        let args = make_args(&[(
            "tag",
            Value::Array(vec![
                Value::String("foo".to_string()),
                Value::String("bar".to_string()),
            ]),
        )]);

        let result = execute_command(&resolved, &args).await;
        let text = result.content[0].as_text().unwrap().text.trim();
        assert_eq!(text, "--tag foo,bar");
    }

    #[tokio::test]
    async fn test_array_args_space_positional() {
        let resolved = ResolvedCommand {
            executable: "echo".to_string(),
            command_parts: vec![],
            args: vec![ArgConfig {
                name: "packages".to_string(),
                arg_type: ArgType::Positional,
                description: "Packages".to_string(),
                required: true,
                json_type: JsonType::Array,
                array_style: ArrayStyle::Space,
            }],
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        let args = make_args(&[(
            "packages",
            Value::Array(vec![
                Value::String("react".to_string()),
                Value::String("vue".to_string()),
            ]),
        )]);

        let result = execute_command(&resolved, &args).await;
        let text = result.content[0].as_text().unwrap().text.trim();
        assert_eq!(text, "react vue");
    }

    #[tokio::test]
    async fn test_failing_command() {
        let resolved = ResolvedCommand {
            executable: "sh".to_string(),
            command_parts: vec![
                "-c".to_string(),
                "echo 'error output' >&2; exit 1".to_string(),
            ],
            args: vec![],
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        let args = serde_json::Map::new();

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("error output"));
    }

    #[tokio::test]
    async fn test_output_schema_valid_json() {
        let resolved = ResolvedCommand {
            executable: "echo".to_string(),
            command_parts: vec![r#"{"name":"test","version":"1.0"}"#.to_string()],
            args: vec![],
            output_schema: Some(serde_json::json!({"type": "object"}).as_object().unwrap().clone()),
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        let args = serde_json::Map::new();

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(false));
        assert!(result.structured_content.is_some());
        let sc = result.structured_content.unwrap();
        let obj = sc.as_object().unwrap();
        assert_eq!(obj["name"], "test");
    }

    #[tokio::test]
    async fn test_output_schema_invalid_json() {
        let resolved = ResolvedCommand {
            executable: "echo".to_string(),
            command_parts: vec!["not json output".to_string()],
            args: vec![],
            output_schema: Some(serde_json::json!({"type": "object"}).as_object().unwrap().clone()),
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        let args = serde_json::Map::new();

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("not valid JSON"));
    }

    #[tokio::test]
    async fn test_nonexistent_command() {
        let resolved = ResolvedCommand {
            executable: "definitely_not_a_real_command_xyz".to_string(),
            command_parts: vec![],
            args: vec![],
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };
        let args = serde_json::Map::new();

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("Failed to execute"));
    }

    #[tokio::test]
    async fn test_env_vars_passed() {
        let mut env = HashMap::new();
        env.insert("TEST_CLI_MCP_VAR".to_string(), "hello_world".to_string());
        let resolved = ResolvedCommand {
            executable: "sh".to_string(),
            command_parts: vec!["-c".to_string(), "echo $TEST_CLI_MCP_VAR".to_string()],
            args: vec![],
            output_schema: None,
            raw_args: vec![],
            env,
            working_dir: None,
        };
        let args = serde_json::Map::new();

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().unwrap().text.trim();
        assert_eq!(text, "hello_world");
    }

    #[tokio::test]
    async fn test_working_dir() {
        let resolved = ResolvedCommand {
            executable: "pwd".to_string(),
            command_parts: vec![],
            args: vec![],
            output_schema: None,
            raw_args: vec![],
            env: HashMap::new(),
            working_dir: Some(PathBuf::from("/tmp")),
        };
        let args = serde_json::Map::new();

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().unwrap().text.trim();
        assert!(text.starts_with("/tmp"));
    }

    #[tokio::test]
    async fn test_raw_args_appended() {
        let resolved = ResolvedCommand {
            executable: "echo".to_string(),
            command_parts: vec!["base".to_string()],
            args: vec![],
            output_schema: None,
            raw_args: vec!["--json".to_string(), "--verbose".to_string()],
            env: HashMap::new(),
            working_dir: None,
        };
        let args = serde_json::Map::new();

        let result = execute_command(&resolved, &args).await;
        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().unwrap().text.trim();
        assert_eq!(text, "base --json --verbose");
    }
}
