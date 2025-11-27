use std::env;
use std::fs;
use std::io::{self, Write};

mod generate_trigger;

use generate_trigger::{
    GenerateTriggerError, find_operation_by_id, generate_executor_code, generate_input_schema,
    generate_output_schema, to_snake_case,
};

fn main() -> Result<(), GenerateTriggerError> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("Usage: generate_trigger <openapi_url> <operation_id> [trigger_name]");
        println!("Example: generate_trigger https://api.example.com/openapi.yaml getUsers");
        println!("Example: generate_trigger https://api.example.com/openapi.yaml getUsers my_custom_name");
        println!("\nTo see available endpoints, run: endpoints <openapi_url>");
        println!("\nIf trigger_name is omitted, it will be derived from operation_id (snake_case).");
        return Ok(());
    }

    let openapi_url = &args[1];
    let operation_id = &args[2];
    let trigger_name = if args.len() >= 4 {
        let provided_name = &args[3];
        // Validate that the provided name is in snake_case format
        if !provided_name.chars().all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_') {
            println!("Error: trigger_name must be in snake_case format (lowercase letters, digits, and underscores only)");
            return Ok(());
        }
        provided_name.to_string()
    } else {
        to_snake_case(operation_id)
    };
    println!("Generating trigger for operationId: {}", operation_id);
    println!("Using OpenAPI spec from: {}", openapi_url);

    // Download the schema
    let schema_yaml = reqwest::blocking::get(openapi_url)?.text()?;

    // Parse YAML and convert to JSON
    let schema_value: serde_yaml::Value = serde_yaml::from_str(&schema_yaml)?;
    let schema_json: serde_json::Value =
        serde_json::from_value(serde_json::to_value(schema_value)?)?;

    // Find the operation by operationId
    let (path, method) = find_operation_by_id(&schema_json, operation_id)?;
    println!("Using endpoint: {} {}", &method, &path);

    println!("Using trigger name: {}", trigger_name);

    // Check if trigger already exists
    let trigger_dir = format!("src/triggers/{}", trigger_name);
    if fs::metadata(&trigger_dir).is_ok() {
        print!(
            "⚠️  Warning: Trigger '{}' already exists. This will overwrite existing files. Continue? (y/N): ",
            trigger_name
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
    fs::create_dir_all(&trigger_dir)?;

    // Generate input schema
    println!("Generating input schema...");
    let input_schema = generate_input_schema(&schema_json, &path, &method)?;
    let input_file = format!("{}/input_schema.json", trigger_dir);
    fs::write(&input_file, serde_json::to_string_pretty(&input_schema)?)?;
    println!("Generated: {}", input_file);

    // Generate output schema
    println!("Generating output schema...");
    let output_schema = generate_output_schema(&schema_json, &path, &method)?;
    let output_file = format!("{}/output_schema.json", trigger_dir);
    fs::write(&output_file, serde_json::to_string_pretty(&output_schema)?)?;
    println!("Generated: {}", output_file);

    // Generate trigger
    println!("Generating trigger...");
    let trigger_code = generate_executor_code(&schema_json, &trigger_name, &path, &method, &path)?;
    let trigger_file = format!("{}/fetch_events.rs", trigger_dir);
    fs::write(&trigger_file, trigger_code)?;
    println!("Generated: {}", trigger_file);

    // Write metadata file to track operation_id
    let metadata = serde_json::json!({
        "operation_id": operation_id,
        "path": path,
        "method": method,
        "trigger_name": trigger_name
    });
    let metadata_file = format!("{}/.metadata.json", trigger_dir);
    fs::write(&metadata_file, serde_json::to_string_pretty(&metadata)?)?;

    println!("✅ Trigger '{}' generated successfully!", trigger_name);
    println!("   - Input schema: {}", input_file);
    println!("   - Output schema: {}", output_file);
    println!("   - Trigger: {}", trigger_file);

    Ok(())
}
