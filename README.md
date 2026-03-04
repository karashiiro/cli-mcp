# cli-mcp

A config-driven CLI-to-MCP server bridge. Expose any command-line tool as an [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) server over stdio transport, letting AI agents call CLI commands with structured inputs and outputs.

## Quick Start

1. Create a config file for your CLI tool:

```toml
[cli]
name = "npm"
executable = "npm"

[[tools]]
command = ["install"]
description = "Install a package"

[[tools.args]]
name = "package"
arg_type = "positional"
description = "Package name"
required = true
json_type = "string"

[[tools]]
command = ["run"]
description = "Run a script"

[[tools.args]]
name = "script"
arg_type = "positional"
description = "Script name"
required = true
json_type = "string"
```

2. Run the server:

```bash
cli-mcp --config npm.toml
```

The server communicates over stdin/stdout using the MCP JSON-RPC protocol.

## How It Works

cli-mcp reads a TOML config describing a CLI's commands and arguments, then:

- Generates an MCP tool for each `[[tools]]` entry (e.g., `npm_install`, `npm_run`, `npm_config_set`)
- Builds JSON Schema `inputSchema` from argument definitions
- Optionally attaches `outputSchema` for commands that produce JSON
- Executes commands via `tokio::process::Command` when tools are called
- Returns text content (or structured JSON if `output_schema` is declared)
- Uses exit codes to determine success/failure

## Features

- **Flat config**: Each tool is independently defined — no recursive nesting, no implicit assumptions
- **All argument types**: Positional, flag (`--name`), option (`--name value`)
- **Array arguments**: Configurable expansion — repeated (`--tag a --tag b`), comma (`--tag a,b`), or space-separated
- **Structured output**: Tools with `output_schema` parse stdout as JSON and return `structuredContent`
- **Environment variables**: Set at CLI or tool level (tool overrides CLI)
- **Working directory**: Configurable per-CLI or per-tool

## Documentation

- [Configuration Reference](docs/configuration.md) — Complete config format with examples
- [Design Document](docs/design.md) — Architecture, design decisions, error handling

## Building

```bash
cargo build --release
```

## Testing

```bash
cargo test
```

Tests include unit tests for each module and integration tests using npm as a real CLI.

## License

MIT
