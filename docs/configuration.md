# cli-mcp Configuration Reference

## Overview

cli-mcp uses TOML configuration files to define how a CLI tool is exposed as an MCP server. Each config file describes one CLI executable and its available commands as MCP tools.

## Config Structure

```toml
[cli]
# Required: CLI metadata
name = "mycli"
executable = "/usr/bin/mycli"

[[tools]]
# Required: One or more tool definitions
command = ["subcommand"]
description = "What this tool does"
```

## `[cli]` Section

The `[cli]` section defines metadata about the CLI being wrapped.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Name used as prefix for tool names (e.g., `"npm"` → tools named `npm_install`, `npm_run`) |
| `executable` | string | yes | Path or name of the CLI executable |
| `env` | table | no | Environment variables to set for all commands |
| `working_dir` | string | no | Working directory for all commands |

### Example

```toml
[cli]
name = "npm"
executable = "npm"
working_dir = "/path/to/project"

[cli.env]
NODE_ENV = "production"
```

## `[[tools]]` Section

Each `[[tools]]` entry defines one MCP tool corresponding to one CLI command invocation.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `command` | array of strings | yes | Command path parts (e.g., `["config", "set"]`) |
| `description` | string | yes | Human-readable description shown to MCP clients |
| `args` | array of tables | no | Argument definitions (see below) |
| `output_schema` | string | no | JSON Schema string for structured output parsing |
| `raw_args` | array of strings | no | Extra arguments always appended to the command |
| `env` | table | no | Environment variables (merged with cli-level, tool wins) |
| `working_dir` | string | no | Working directory (overrides cli-level) |

### Tool Naming

Tool names are automatically generated:

```
{cli.name}_{command[0]}_{command[1]}_{...}
```

Examples:
- `command = ["install"]` with `name = "npm"` → `npm_install`
- `command = ["config", "set"]` with `name = "npm"` → `npm_config_set`
- `command = ["remote", "add"]` with `name = "git"` → `git_remote_add`

### Example

```toml
[[tools]]
command = ["config", "set"]
description = "Set a configuration key to a value"
env = { DEBUG = "true" }

[[tools.args]]
name = "key"
arg_type = "positional"
description = "Configuration key"
required = true
json_type = "string"

[[tools.args]]
name = "value"
arg_type = "positional"
description = "Configuration value"
required = true
json_type = "string"
```

## `[[tools.args]]` Section

Each argument definition describes one parameter for the CLI command.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Argument name (used for flag names and JSON Schema property keys) |
| `arg_type` | string | yes | How the argument is passed: `"positional"`, `"flag"`, or `"option"` |
| `description` | string | yes | Human-readable description for the JSON Schema |
| `required` | boolean | no | Whether the argument is required (default: `false`) |
| `json_type` | string | yes | JSON Schema type: `"string"`, `"number"`, `"integer"`, `"boolean"`, `"array"` |
| `array_style` | string | no | For array args: `"repeated"`, `"comma"`, or `"space"` (default: `"repeated"`) |

### Argument Types

#### `positional`
Bare values appended to the command in the order they appear in config.

```toml
# npm install react
[[tools.args]]
name = "package"
arg_type = "positional"
description = "Package name"
required = true
json_type = "string"
```

#### `flag`
Boolean flags. When `true`, `--name` is appended. When `false` or omitted, nothing is added.

```toml
# npm install --save-dev
[[tools.args]]
name = "save-dev"
arg_type = "flag"
description = "Save as dev dependency"
required = false
json_type = "boolean"
```

#### `option`
Named options with values. Appended as `--name value`.

```toml
# npm install --registry https://registry.npmjs.org
[[tools.args]]
name = "registry"
arg_type = "option"
description = "Registry URL"
required = false
json_type = "string"
```

### JSON Types

| Type | JSON Schema | Description |
|------|-------------|-------------|
| `string` | `"type": "string"` | Text value |
| `number` | `"type": "number"` | Floating-point number |
| `integer` | `"type": "integer"` | Whole number |
| `boolean` | `"type": "boolean"` | True/false (used with flag args) |
| `array` | `"type": "array"` | List of values (see array styles) |

### Array Styles

For arguments with `json_type = "array"`, the `array_style` field controls how array values are passed to the CLI:

| Style | Example Input | CLI Output |
|-------|---------------|------------|
| `repeated` (default) | `["foo", "bar"]` | `--tag foo --tag bar` |
| `comma` | `["foo", "bar"]` | `--tag foo,bar` |
| `space` | `["foo", "bar"]` | `foo bar` (for positional) |

```toml
[[tools.args]]
name = "tag"
arg_type = "option"
description = "Tags to apply"
required = false
json_type = "array"
array_style = "repeated"
```

### Property Name Mapping

Argument names with dashes are converted to underscores in the JSON Schema property names:
- Arg name `save-dev` → JSON Schema property `save_dev`
- The CLI flag remains `--save-dev`

This allows MCP clients to use valid JSON property names while the CLI receives the correct flag format.

## Output Schema

When a tool declares `output_schema`, the server will:
1. Trim and parse the command's stdout as JSON
2. Return it as `structuredContent` in the MCP response
3. Return an error if stdout is not valid JSON

```toml
[[tools]]
command = ["view"]
description = "View package info as JSON"
raw_args = ["--json"]
output_schema = '''
{
  "type": "object",
  "properties": {
    "name": { "type": "string" },
    "version": { "type": "string" },
    "description": { "type": "string" }
  }
}
'''

[[tools.args]]
name = "package"
arg_type = "positional"
description = "Package name"
required = true
json_type = "string"
```

The `output_schema` value must be a valid JSON string containing a JSON Schema object.

## Raw Arguments

Use `raw_args` to append fixed arguments to every invocation of a tool:

```toml
[[tools]]
command = ["view"]
description = "View package info"
raw_args = ["--json", "--long"]
```

Raw arguments are appended after all user-provided arguments.

## Environment Variables

Environment variables can be set at two levels:

```toml
# CLI-level: applies to all tools
[cli.env]
NODE_ENV = "production"

# Tool-level: applies to this tool only (overrides cli-level)
[[tools]]
command = ["build"]
description = "Build the project"

[tools.env]
NODE_ENV = "development"
DEBUG = "true"
```

When both levels define the same variable, the tool-level value wins.

## Working Directory

```toml
[cli]
working_dir = "/default/project/path"

[[tools]]
command = ["test"]
description = "Run tests"
working_dir = "/specific/test/path"  # overrides cli-level
```

## Complete Example: npm

```toml
[cli]
name = "npm"
executable = "npm"

[[tools]]
command = ["install"]
description = "Install packages from package.json or by name"

[[tools.args]]
name = "package"
arg_type = "positional"
description = "Package name (optional, installs from package.json if omitted)"
required = false
json_type = "string"

[[tools.args]]
name = "save-dev"
arg_type = "flag"
description = "Save package as a dev dependency"
required = false
json_type = "boolean"

[[tools.args]]
name = "global"
arg_type = "flag"
description = "Install package globally"
required = false
json_type = "boolean"

[[tools]]
command = ["run"]
description = "Run a script defined in package.json"

[[tools.args]]
name = "script"
arg_type = "positional"
description = "Script name to run"
required = true
json_type = "string"

[[tools]]
command = ["config", "set"]
description = "Set a config key to a value"

[[tools.args]]
name = "key"
arg_type = "positional"
description = "Config key"
required = true
json_type = "string"

[[tools.args]]
name = "value"
arg_type = "positional"
description = "Config value"
required = true
json_type = "string"

[[tools]]
command = ["config", "get"]
description = "Get a config value"

[[tools.args]]
name = "key"
arg_type = "positional"
description = "Config key"
required = true
json_type = "string"

[[tools]]
command = ["view"]
description = "View package info as JSON"
raw_args = ["--json"]
output_schema = """
{
  "type": "object",
  "properties": {
    "name": { "type": "string" },
    "version": { "type": "string" }
  }
}
"""

[[tools.args]]
name = "package"
arg_type = "positional"
description = "Package name"
required = true
json_type = "string"
```

## Complete Example: git

```toml
[cli]
name = "git"
executable = "git"

[[tools]]
command = ["status"]
description = "Show the working tree status"

[[tools.args]]
name = "short"
arg_type = "flag"
description = "Give output in short format"
required = false
json_type = "boolean"

[[tools]]
command = ["add"]
description = "Add file contents to the index"

[[tools.args]]
name = "files"
arg_type = "positional"
description = "Files to add"
required = true
json_type = "array"
array_style = "space"

[[tools]]
command = ["commit"]
description = "Record changes to the repository"

[[tools.args]]
name = "message"
arg_type = "option"
description = "Commit message"
required = true
json_type = "string"

[[tools.args]]
name = "all"
arg_type = "flag"
description = "Automatically stage modified and deleted files"
required = false
json_type = "boolean"

[[tools]]
command = ["remote", "add"]
description = "Add a new remote"

[[tools.args]]
name = "name"
arg_type = "positional"
description = "Remote name"
required = true
json_type = "string"

[[tools.args]]
name = "url"
arg_type = "positional"
description = "Remote URL"
required = true
json_type = "string"

[[tools]]
command = ["log"]
description = "Show commit logs"

[[tools.args]]
name = "n"
arg_type = "option"
description = "Number of commits to show"
required = false
json_type = "integer"

[[tools.args]]
name = "oneline"
arg_type = "flag"
description = "Show each commit on a single line"
required = false
json_type = "boolean"
```
