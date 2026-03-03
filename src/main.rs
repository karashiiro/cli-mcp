mod config;
mod executor;
mod server;
mod tool_gen;

use clap::Parser;
use rmcp::ServiceExt;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "cli-mcp", about = "Config-driven CLI-to-MCP server bridge")]
struct Cli {
    /// Path to the TOML configuration file
    #[arg(long)]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    // Load config
    let config_str = std::fs::read_to_string(&cli.config).map_err(|e| {
        format!(
            "Failed to read config file '{}': {}",
            cli.config.display(),
            e
        )
    })?;

    let config = config::parse_config(&config_str)
        .map_err(|e| format!("Failed to parse config: {}", e))?;

    // Generate tools
    let generated = tool_gen::generate_tools(&config)
        .map_err(|e| format!("Failed to generate tools: {}", e))?;

    tracing::info!(
        "Loaded {} tools for CLI '{}'",
        generated.tools.len(),
        config.cli.name
    );

    // Create and start server
    let server = server::CliMcpServer::new(
        format!("cli-mcp-{}", config.cli.name),
        generated.tools,
        generated.commands,
    );

    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
