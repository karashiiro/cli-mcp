//! End-to-end integration tests using npm as the test CLI.
//!
//! These tests load the npm config fixture, generate tools, and execute
//! real npm commands to verify the full pipeline works.

use cli_mcp::config;
use cli_mcp::server::CliMcpServer;
use cli_mcp::tool_gen::generate_tools;

fn load_npm_config() -> config::CliConfig {
    let toml_str = std::fs::read_to_string("tests/fixtures/npm.toml")
        .expect("Failed to read npm.toml fixture");
    config::parse_config(&toml_str).expect("Failed to parse npm.toml")
}

fn make_npm_server() -> CliMcpServer {
    let config = load_npm_config();
    let generated = generate_tools(&config).unwrap();
    CliMcpServer::new(
        format!("cli-mcp-{}", config.cli.name),
        generated.tools,
        generated.commands,
    )
}

#[test]
fn test_npm_config_loads_correct_tool_count() {
    let config = load_npm_config();
    let generated = generate_tools(&config).unwrap();

    // Should have 8 tools defined in npm.toml
    assert_eq!(generated.tools.len(), 8);
}

#[test]
fn test_npm_tool_names() {
    let config = load_npm_config();
    let generated = generate_tools(&config).unwrap();

    let names: Vec<&str> = generated.tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(names.contains(&"npm_--version"));
    assert!(names.contains(&"npm_install"));
    assert!(names.contains(&"npm_run"));
    assert!(names.contains(&"npm_config_list"));
    assert!(names.contains(&"npm_config_set"));
    assert!(names.contains(&"npm_config_get"));
    assert!(names.contains(&"npm_view"));
    assert!(names.contains(&"npm_help"));
}

#[test]
fn test_npm_view_has_output_schema() {
    let config = load_npm_config();
    let generated = generate_tools(&config).unwrap();

    let view_tool = generated
        .tools
        .iter()
        .find(|t| t.name.as_ref() == "npm_view")
        .expect("npm_view tool not found");
    assert!(view_tool.output_schema.is_some());
}

#[test]
fn test_npm_install_has_correct_schema() {
    let config = load_npm_config();
    let generated = generate_tools(&config).unwrap();

    let install_tool = generated
        .tools
        .iter()
        .find(|t| t.name.as_ref() == "npm_install")
        .expect("npm_install tool not found");

    let schema = install_tool.schema_as_json_value();
    let props = schema["properties"].as_object().unwrap();

    assert!(props.contains_key("package"));
    assert!(props.contains_key("save_dev"));
    assert!(props.contains_key("global"));
    assert!(props.contains_key("registry"));

    assert_eq!(props["package"]["type"], "string");
    assert_eq!(props["save_dev"]["type"], "boolean");
    assert_eq!(props["global"]["type"], "boolean");
    assert_eq!(props["registry"]["type"], "string");
}

#[tokio::test]
async fn test_npm_version_execution() {
    let server = make_npm_server();

    let result = server
        .handle_call("npm_--version", serde_json::Map::new())
        .await;
    assert_eq!(result.is_error, Some(false));
    let text = result.content[0].as_text().unwrap().text.trim();
    // npm --version should output something like "10.9.4"
    assert!(
        text.contains('.'),
        "Expected version string with dots, got: {}",
        text
    );
}

#[tokio::test]
async fn test_npm_help_execution() {
    let server = make_npm_server();

    let result = server
        .handle_call("npm_help", serde_json::Map::new())
        .await;
    assert_eq!(result.is_error, Some(false));
    let text = result.content[0].as_text().unwrap().text.clone();
    // npm help should contain "npm" somewhere in output
    assert!(
        text.to_lowercase().contains("npm"),
        "Expected npm help output, got: {}",
        text
    );
}

#[tokio::test]
async fn test_npm_config_get() {
    let server = make_npm_server();

    let mut args = serde_json::Map::new();
    args.insert(
        "key".to_string(),
        serde_json::Value::String("registry".to_string()),
    );

    let result = server.handle_call("npm_config_get", args).await;
    assert_eq!(result.is_error, Some(false));
    let text = result.content[0].as_text().unwrap().text.trim();
    // Should return a URL like https://registry.npmjs.org/
    assert!(
        text.contains("registry"),
        "Expected registry URL, got: {}",
        text
    );
}

#[tokio::test]
async fn test_nonexistent_tool_error() {
    let server = make_npm_server();

    let result = server
        .handle_call("npm_totally_fake", serde_json::Map::new())
        .await;
    assert_eq!(result.is_error, Some(true));
    let text = result.content[0].as_text().unwrap().text.clone();
    assert!(text.contains("Unknown tool"));
}

#[tokio::test]
async fn test_npm_view_structured_content() {
    let server = make_npm_server();

    let mut args = serde_json::Map::new();
    args.insert(
        "package".to_string(),
        serde_json::Value::String("is-odd".to_string()),
    );

    let result = server.handle_call("npm_view", args).await;
    // npm view is-odd --json should return valid JSON with structured content
    assert_eq!(
        result.is_error,
        Some(false),
        "npm view failed: {:?}",
        result.content
    );
    assert!(
        result.structured_content.is_some(),
        "Expected structured content for npm_view"
    );

    let sc = result.structured_content.unwrap();
    // The structured content should be a JSON object with at least "name"
    let obj = sc.as_object().unwrap();
    assert_eq!(obj["name"], "is-odd");
}
