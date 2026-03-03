mod config;
mod executor;
mod server;
mod tool_gen;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "cli-mcp", about = "Config-driven CLI-to-MCP server bridge")]
struct Cli {
    /// Path to the TOML configuration file
    #[arg(long)]
    config: PathBuf,
}

fn main() {
    let _cli = Cli::parse();
    // Will be implemented in Phase 4
    todo!("MCP server startup")
}
