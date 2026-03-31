use std::env;
use std::fs;
use std::io::{self, Write};

mod generate_action;
use base_connector_tools::parse_api_spec;
use generate_action::{
    GenerateActionError, find_operation_by_id, generate_executor_code, generate_input_schema,
    generate_output_schema, to_snake_case,
};

fn main() -> Result<(), GenerateActionError> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!(
            "Usage: generate_action <openapi_or_postman_url_or_file> <operation_id> [action_name]"
        );
        println!("Example: generate_action https://api.example.com/openapi.yaml getUsers");
        println!("Example: generate_action ./openapi.yaml getUsers");
        println!(
            "Example: generate_action https://api.example.com/openapi.yaml getUsers my_custom_name"
        );
        println!("Example: generate_action https://example.com/api/collection.json list_users");
        println!("\nTo see available endpoints, run: endpoints <openapi_or_postman_url_or_file>");
        println!("\nIf action_name is omitted, it will be derived from operation_id (snake_case).");
        return Ok(());
    }

    let openapi_url = &args[1];
    let operation_id = &args[2];
    let action_name = if args.len() >= 4 {
        let provided_name = &args[3];
        // Validate that the provided name is in snake_case format
        if !provided_name
            .chars()
            .all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_')
        {
            println!(
                "Error: action_name must be in snake_case format (lowercase letters, digits, and underscores only)"
            );
            return Ok(());
        }
        provided_name.to_string()
    } else {
        to_snake_case(operation_id)
    };
    println!("Generating action for operationId: {}", operation_id);
    if openapi_url.starts_with("http://") || openapi_url.starts_with("https://") {
        println!("Fetching API description from: {}", openapi_url);
    } else {
        println!("Reading API description from: {}", openapi_url);
    }

    // Load OpenAPI YAML/JSON or Postman Collection JSON
    let spec_raw = if openapi_url.starts_with("http://") || openapi_url.starts_with("https://") {
        reqwest::blocking::get(openapi_url)?.text()?
    } else {
        fs::read_to_string(openapi_url)?
    };

    let schema_json =
        parse_api_spec(&spec_raw).map_err(|e| GenerateActionError::SchemaError(e.to_string()))?;

    // Find the operation by operationId
    let (path, method, api_path) = find_operation_by_id(&schema_json, operation_id)?;
    println!("Using endpoint: {} {}", &method, &api_path);

    println!("Using action name: {}", action_name);

    // Check if action already exists
    let action_dir = format!("src/actions/{}", action_name);
    if fs::metadata(&action_dir).is_ok() {
        print!(
            "⚠️  Warning: Action '{}' already exists. This will overwrite existing files. Continue? (y/N): ",
            action_name
        );
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;

        let response = response.trim().to_lowercase();
        if response != "y" && response != "yes" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Create directory structure
    fs::create_dir_all(&action_dir)?;

    // Generate input schema
    println!("Generating input schema...");
    let input_schema = generate_input_schema(&schema_json, &path, &method)?;
    let input_file = format!("{}/base_input_schema.json", action_dir);
    fs::write(&input_file, serde_json::to_string_pretty(&input_schema)?)?;
    println!("Generated: {}", input_file);

    // Generate output schema
    println!("Generating output schema...");
    let output_schema = generate_output_schema(&schema_json, &path, &method)?;
    let output_file = format!("{}/base_output_schema.json", action_dir);
    fs::write(&output_file, serde_json::to_string_pretty(&output_schema)?)?;
    println!("Generated: {}", output_file);

    // Generate action
    println!("Generating action...");
    let action_code =
        generate_executor_code(&schema_json, &action_name, &path, &method, &api_path)?;
    let action_file = format!("{}/action.rs", action_dir);
    fs::write(&action_file, action_code)?;
    println!("Generated: {}", action_file);

    // Write metadata file to track operation_id
    let metadata = serde_json::json!({
        "operation_id": operation_id,
        "path": api_path,
        "method": method,
        "action_name": action_name
    });
    let metadata_file = format!("{}/.metadata.json", action_dir);
    fs::write(&metadata_file, serde_json::to_string_pretty(&metadata)?)?;

    println!("✅ Action '{}' generated successfully!", action_name);
    println!("   - Input schema: {}", input_file);
    println!("   - Output schema: {}", output_file);
    println!("   - Action: {}", action_file);

    Ok(())
}
