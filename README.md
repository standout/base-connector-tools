# Base Connector Tools

Development tools for generating connector actions and triggers from **OpenAPI** specifications or **Postman Collection** JSON (v2.x).

## Installation

### From Git Repository

```bash
cargo install --git https://github.com/standout/base-connector-tools.git --bin endpoints
cargo install --git https://github.com/standout/base-connector-tools.git --bin generate_action
cargo install --git https://github.com/standout/base-connector-tools.git --bin generate_trigger
cargo install --git https://github.com/standout/base-connector-tools.git --bin generate_connection_strategy
cargo install --git https://github.com/standout/base-connector-tools.git --bin connection_strategies
```

### From Local Source

```bash
git clone https://github.com/standout/base-connector-tools.git
cd base-connector-tools
cargo install --path . --bin endpoints
cargo install --path . --bin generate_action
cargo install --path . --bin generate_trigger
cargo install --path . --bin generate_connection_strategy
cargo install --path . --bin connection_strategies
```

## Usage

The same commands accept an OpenAPI YAML/JSON file or URL, or a Postman Collection JSON file or URL. For Postman, each request becomes an operation; the **operation ID** shown by `endpoints` is derived from folder and request names (see `endpoints` output).

### Postman collections: limitations

Postman is supported for discovery and codegen, but collections carry much less structure than OpenAPI:

- **No response schema** — Collections do not define response body shapes. Generated output schemas are intentionally open (any JSON object) with a note to refine them from docs or real responses.
- **Input schema is often incomplete** — It is inferred from saved **raw** bodies (when they are valid JSON), query params, and path templates. Bodies that are plain text, empty, or invalid JSON become a generic `body` string field. Postman `{{variables}}` in JSON are approximated (replaced with `null` for parsing) so keys can still be inferred; values may be wrong until you edit the schema.
- **Same URL + method** — Many requests can share one path (e.g. RPC-style APIs). Each request still gets a unique internal ID; use the operation ID from `endpoints`, not only the path.

Prefer **OpenAPI** when you need accurate request/response schemas; use **Postman** when that is all you have, then tighten `base_input_schema.json` / `base_output_schema.json` (or `input_schema.json` / `output_schema.json` for triggers) by hand.

### Endpoints

Discover available operations from an OpenAPI specification or Postman collection:

```bash
endpoints <openapi_or_postman_url_or_file>
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
generate_action <openapi_or_postman_url_or_file> <operation_id> [action_name]
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
- `<openapi_or_postman_url_or_file>` - URL or path to an OpenAPI spec or Postman Collection JSON
- `<operation_id>` - The operation ID from the spec (`operationId` in OpenAPI, or the ID printed by `endpoints` for Postman)
- `[action_name]` - (Optional) Custom name for the action in snake_case format. If omitted, the name will be derived from `operation_id`.

This will:
- Generate `base_input_schema.json` - Input schema for the action
- Generate `base_output_schema.json` - Output schema for the action
- Generate `action.rs` - Rust code for executing the action

The generated files will be placed in `src/actions/<action_name>/` directory.

### Connection Strategies

List OAuth metadata and suggested strategies before generating config:

```bash
# Prefer --issuer for most SaaS (few APIs publish RFC 9728 protected-resource metadata)
connection_strategies --issuer https://accounts.google.com
connection_strategies --issuer https://accounts.google.com oauth2
connection_strategies --issuer https://accounts.google.com --json
```

With a strategy id, the tool prints a ready-to-run `generate_connection_strategy` command.

### Generate Connection Strategy

Generate `connection-config.json` from an OAuth issuer's well-known metadata (RFC 8414 / OIDC discovery):

```bash
generate_connection_strategy \
  --api-base-url https://www.googleapis.com \
  --issuer https://accounts.google.com \
  --client-id-env GOOGLE_CLIENT_ID \
  --client-secret-env GOOGLE_CLIENT_SECRET \
  --scopes https://www.googleapis.com/auth/drive.readonly \
  --post-connect-path /drive/v3/about?fields=user \
  -o src/connection-config.json
```

This fetches OAuth discovery metadata at build time:

1. `GET {api}/.well-known/oauth-protected-resource` (RFC 9728) when available
2. `authorization_servers[0]` → issuer
3. `GET {issuer}/.well-known/oauth-authorization-server` or `openid-configuration`

`--issuer` is required for most real APIs. Output matches app_bridge connection-config schema v2 (`authorization_server` + `oauth_client`, Bearer runtime headers, optional `post_connect`).

If `-o` already exists, the new strategy is **appended** (same `id` replaces the old entry). Use `--replace` to overwrite the file, `--all-strategies` to generate every suggested grant in one go, and `--set-default` to update `default_strategy`.

```bash
# Append another strategy (e.g. different scopes / id) to an existing config
generate_connection_strategy \
  --api-base-url https://www.googleapis.com \
  --issuer https://accounts.google.com \
  --strategy-id oauth2_calendar \
  --client-id-env GOOGLE_CLIENT_ID \
  --client-secret-env GOOGLE_CLIENT_SECRET \
  --scopes https://www.googleapis.com/auth/calendar.readonly \
  -o src/connection-config.json

# Or generate all suggested strategies at once
generate_connection_strategy \
  --api-base-url https://www.googleapis.com \
  --issuer https://accounts.google.com \
  --client-id-env GOOGLE_CLIENT_ID \
  --client-secret-env GOOGLE_CLIENT_SECRET \
  --all-strategies \
  -o src/connection-config.json
```

The connector can export the file via `include_str!("connection-config.json")` in `connections.rs`.

### Generate Trigger

Generate trigger code and schemas for a specific operation:

```bash
generate_trigger <openapi_or_postman_url_or_file> <operation_id> [trigger_name]
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
- `<openapi_or_postman_url_or_file>` - URL or path to an OpenAPI spec or Postman Collection JSON
- `<operation_id>` - The operation ID from the spec or from `endpoints` (Postman)
- `[trigger_name]` - (Optional) Custom name for the trigger in snake_case format. If omitted, the name will be derived from `operation_id`.

This will:
- Generate `input_schema.json` - Input schema for the trigger (typically empty)
- Generate `output_schema.json` - Output schema for each event (for Postman sources, see limitations above—no response schema in the collection)
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
- `target/release/generate_connection_strategy`
- `target/release/connection_strategies`
