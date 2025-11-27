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
endpoints <openapi_url>
```

**Example:**
```bash
endpoints https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.json
```

This will list all available operations with their HTTP methods and paths.

### Generate Action

Generate action code and schemas for a specific operation:

```bash
generate_action <openapi_url> <operation_id>
```

**Example:**
```bash
generate_action https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.json repos/get
```

This will:
- Generate `base_input_schema.json` - Input schema for the action
- Generate `base_output_schema.json` - Output schema for the action
- Generate `action.rs` - Rust code for executing the action

The generated files will be placed in `src/actions/<action_name>/` directory.

### Generate Trigger

Generate trigger code and schemas for a specific operation:

```bash
generate_trigger <openapi_url> <operation_id>
```

**Example:**
```bash
generate_trigger https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.json repos/list
```

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
