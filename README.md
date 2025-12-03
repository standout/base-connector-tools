# Base Connector Tools

Development tools for generating connector actions and triggers from OpenAPI specifications.

## Installation

### From Git Repository

```bash
cargo install --git https://github.com/standout/base-connector-tools.git --bin endpoints
cargo install --git https://github.com/standout/base-connector-tools.git --bin generate_action
cargo install --git https://github.com/standout/base-connector-tools.git --bin generate_trigger
```

### From Local Source

```bash
git clone https://github.com/standout/base-connector-tools.git
cd base-connector-tools
cargo install --path . --bin endpoints
cargo install --path . --bin generate_action
cargo install --path . --bin generate_trigger
```

## Usage

### Endpoints

Discover available endpoints from an OpenAPI specification:

```bash
endpoints <openapi_url_or_file>
```

**Examples:**
```bash
# From URL
endpoints https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.json

# From local file (relative to current directory)
endpoints ./openapi.yaml
endpoints openapi.json

# From local file (absolute path)
endpoints /path/to/openapi.json
```

**Note:** When using `./example.json` or `example.json`, the file should be in the current working directory where you run the command. You can also use absolute paths like `/path/to/file.json`.

This will list all available operations with their HTTP methods and paths.

### Generate Action

Generate action code and schemas for a specific operation:

```bash
generate_action <openapi_url_or_file> <operation_id> [action_name]
```

**Examples:**
```bash
# From URL (uses default name derived from operation_id)
generate_action https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.json repos/get

# From local file (relative to current directory) with custom name
generate_action ./openapi.yaml repos/get my_custom_action
generate_action openapi.json repos/get

# From local file (absolute path)
generate_action /path/to/openapi.yaml repos/get
```

**Parameters:**
- `<openapi_url_or_file>` - URL or path to OpenAPI specification file
- `<operation_id>` - The operation ID from the OpenAPI spec
- `[action_name]` - (Optional) Custom name for the action in snake_case format. If omitted, the name will be derived from `operation_id`.

This will:
- Generate `base_input_schema.json` - Input schema for the action
- Generate `base_output_schema.json` - Output schema for the action
- Generate `action.rs` - Rust code for executing the action

The generated files will be placed in `src/actions/<action_name>/` directory.

### Generate Trigger

Generate trigger code and schemas for a specific operation:

```bash
generate_trigger <openapi_url_or_file> <operation_id> [trigger_name]
```

**Examples:**
```bash
# From URL (uses default name derived from operation_id)
generate_trigger https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.json repos/list

# From local file (relative to current directory) with custom name
generate_trigger ./openapi.yaml repos/list my_custom_trigger
generate_trigger openapi.json repos/list

# From local file (absolute path)
generate_trigger /path/to/openapi.yaml repos/list
```

**Note:** When using `./example.json` or `example.json`, the file should be in the current working directory where you run the command.

**Parameters:**
- `<openapi_url_or_file>` - URL or path to OpenAPI specification file
- `<operation_id>` - The operation ID from the OpenAPI spec
- `[trigger_name]` - (Optional) Custom name for the trigger in snake_case format. If omitted, the name will be derived from `operation_id`.

This will:
- Generate `input_schema.json` - Input schema for the trigger (typically empty)
- Generate `output_schema.json` - Output schema for each event
- Generate `fetch_events.rs` - Rust code for fetching trigger events

The generated files will be placed in `src/triggers/<trigger_name>/` directory.

**Note:** The generated trigger code includes placeholders for store data handling. You'll need to customize:
- How to read timestamps/state from `context.store`
- How to filter data based on the timestamp ("since" parameter)
- How to process API response into events
- How to update store data with new state

The generated code includes clear comments showing how to read and set store data.

## Development

To build the tools locally:

```bash
cargo build --release
```

The binaries will be available in `target/release/`:
- `target/release/endpoints`
- `target/release/generate_action`
- `target/release/generate_trigger`
