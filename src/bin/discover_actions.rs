use base_connector_tools::to_snake_case;
use serde_json::Value;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage: endpoints <openapi_url> [operation_id]");
        println!("Example: endpoints https://api.example.com/openapi.yaml");
        println!("Example: endpoints https://api.example.com/openapi.yaml getUsers");
        println!("\nIf no operation_id is provided, all available endpoints will be listed.");
        return Ok(());
    }

    let openapi_url = &args[1];

    if args.len() < 3 {
        // No operation_id provided, list all actions
        println!("Fetching OpenAPI spec from: {}", openapi_url);
        print_available_actions(openapi_url)?;
        return Ok(());
    }

    let operation_id = &args[2];
    println!("Discovering action for operationId: {}", operation_id);
    println!("Using OpenAPI spec from: {}", openapi_url);

    // Download the schema from OpenAPI URL
    let schema_yaml = reqwest::blocking::get(openapi_url)?.text()?;
    let schema_value: serde_yaml::Value = serde_yaml::from_str(&schema_yaml)?;
    let schema_json: serde_json::Value =
        serde_json::from_value(serde_json::to_value(schema_value)?)?;

    // Find the operation by operationId
    let (path, method) = find_operation_by_id(&schema_json, operation_id)?;
    println!("Found operation: {} {}", method.to_uppercase(), path);

    // Generate action name from operationId (convert camelCase to snake_case)
    let action_name = to_snake_case(operation_id);
    println!("Generated action name: {}", action_name);

    Ok(())
}

fn find_operation_by_id(
    schema: &Value,
    operation_id: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let paths = schema.get("paths").ok_or("No paths in schema")?;

    for (path, path_obj) in paths.as_object().unwrap() {
        if let Some(path_obj) = path_obj.as_object() {
            for (method, method_obj) in path_obj {
                if let Some(method_obj) = method_obj.as_object()
                    && let Some(op_id) = method_obj.get("operationId")
                    && let Some(op_id_str) = op_id.as_str()
                    && op_id_str == operation_id
                {
                    return Ok((path.clone(), method.clone()));
                }
            }
        }
    }

    Err(format!("Operation with ID '{}' not found in schema", operation_id).into())
}

fn print_available_actions(openapi_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let schema_yaml = reqwest::blocking::get(openapi_url)?.text()?;
    let schema_value: serde_yaml::Value = serde_yaml::from_str(&schema_yaml)?;
    let schema_json: serde_json::Value =
        serde_json::from_value(serde_json::to_value(schema_value)?)?;

    let paths = schema_json.get("paths").ok_or("No paths in schema")?;

    println!("Available operations:");
    println!("🟢 = Action | 🔵 = Trigger | 🟣 = Both | ⚪ = Not implemented");
    println!();

    for (path, path_obj) in paths.as_object().unwrap() {
        if let Some(path_obj) = path_obj.as_object() {
            for (method, method_obj) in path_obj {
                if let Some(method_obj) = method_obj.as_object()
                    && let Some(op_id) = method_obj.get("operationId")
                    && let Some(op_id_str) = op_id.as_str()
                {
                    // Get description from summary or description field
                    let description = method_obj
                        .get("summary")
                        .and_then(|s| s.as_str())
                        .or_else(|| method_obj.get("description").and_then(|d| d.as_str()))
                        .unwrap_or("No description available");

                    // Check if operation is already implemented (as action, trigger, or both)
                    let impl_type = check_operation_implementation(op_id_str);

                    let status_icon = match impl_type {
                        ImplementationType::Action => "🟢",
                        ImplementationType::Trigger => "🔵",
                        ImplementationType::Both => "🟣",
                        ImplementationType::None => "⚪",
                    };

                    println!(
                        "  {} {} - {} {} - {}",
                        status_icon,
                        op_id_str,
                        method.to_uppercase(),
                        path,
                        description
                    );
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ImplementationType {
    None,
    Action,
    Trigger,
    Both,
}

fn is_action_implemented(action_name: &str) -> bool {
    let action_dir = format!("src/actions/{}", action_name);
    let action_path = format!("{}/action.rs", action_dir);

    std::path::Path::new(&action_path).exists()
}

fn is_trigger_implemented(trigger_name: &str) -> bool {
    let trigger_dir = format!("src/triggers/{}", trigger_name);
    let trigger_path = format!("{}/fetch_events.rs", trigger_dir);

    std::path::Path::new(&trigger_path).exists()
}

/// Check if an operation_id is implemented by searching all action and trigger directories
/// Returns the type(s) of implementation found
fn check_operation_implementation(operation_id: &str) -> ImplementationType {
    let mut found_action = false;
    let mut found_trigger = false;

    // First check the default name
    let default_name = to_snake_case(operation_id);
    if is_action_implemented(&default_name) {
        found_action = true;
    }
    if is_trigger_implemented(&default_name) {
        found_trigger = true;
    }

    // Search through all action directories to find if any contain this operation_id
    // by checking metadata files
    let actions_dir = std::path::Path::new("src/actions");
    if actions_dir.exists()
        && let Ok(entries) = std::fs::read_dir(actions_dir)
    {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let metadata_file = entry.path().join(".metadata.json");
                if metadata_file.exists()
                    && let Ok(metadata_content) = std::fs::read_to_string(&metadata_file)
                    && let Ok(metadata) = serde_json::from_str::<Value>(&metadata_content)
                    && let Some(stored_op_id) = metadata.get("operation_id").and_then(|v| v.as_str())
                    && stored_op_id == operation_id
                {
                    found_action = true;
                    break;
                }
            }
        }
    }

    // Search through all trigger directories
    let triggers_dir = std::path::Path::new("src/triggers");
    if triggers_dir.exists()
        && let Ok(entries) = std::fs::read_dir(triggers_dir)
    {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let metadata_file = entry.path().join(".metadata.json");
                if metadata_file.exists()
                    && let Ok(metadata_content) = std::fs::read_to_string(&metadata_file)
                    && let Ok(metadata) = serde_json::from_str::<Value>(&metadata_content)
                    && let Some(stored_op_id) = metadata.get("operation_id").and_then(|v| v.as_str())
                    && stored_op_id == operation_id
                {
                    found_trigger = true;
                    break;
                }
            }
        }
    }

    match (found_action, found_trigger) {
        (true, true) => ImplementationType::Both,
        (true, false) => ImplementationType::Action,
        (false, true) => ImplementationType::Trigger,
        (false, false) => ImplementationType::None,
    }
}
