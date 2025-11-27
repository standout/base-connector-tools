# Base Connector Tools

Development tools for generating connector actions from OpenAPI specifications.

## Installation

### From Git Repository

```bash
cargo install --git https://github.com/standout/base-connector-tools.git --bin discover_actions
cargo install --git https://github.com/standout/base-connector-tools.git --bin generate_action
```

### From Local Source

```bash
git clone https://github.com/standout/base-connector-tools.git
cd base-connector-tools
cargo install --path . --bin discover_actions
cargo install --path . --bin generate_action
```

## Usage

### Discover Actions

Discover available actions from an OpenAPI specification:

```bash
discover_actions <openapi_url>
```

**Example:**
```bash
discover_actions https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.json
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

## Development

To build the tools locally:

```bash
cargo build --release
```

The binaries will be available in `target/release/`:
- `target/release/discover_actions`
- `target/release/generate_action`

## License

MIT OR Apache-2.0

