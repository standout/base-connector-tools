use base_connector_tools::{parse_api_spec, to_snake_case};
use serde_json::Value;
use std::env;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage: endpoints <openapi_or_postman_url_or_file> [operation_id]");
        println!("Example: endpoints https://api.example.com/openapi.yaml");
        println!("Example: endpoints ./openapi.yaml");
        println!("Example: endpoints https://api.example.com/openapi.yaml getUsers");
        println!("\nIf no operation_id is provided, all available endpoints will be listed.");
        return Ok(());
    }

    let openapi_url = &args[1];

    if args.len() < 3 {
        // No operation_id provided, list all endpoints
        print_available_endpoints(openapi_url)?;
        return Ok(());
    }

    let operation_id = &args[2];
    println!("Discovering endpoint for operationId: {}", operation_id);

    let spec_raw = load_schema(openapi_url)?;
    let schema_json = parse_api_spec(&spec_raw)?;

    // Find the operation by operationId
    let (_schema_path, method, api_path) = find_operation_by_id(&schema_json, operation_id)?;
    println!("Found operation: {} {}", method.to_uppercase(), api_path);

    // Generate endpoint name from operationId (convert camelCase to snake_case)
    let endpoint_name = to_snake_case(operation_id);
    println!("Generated endpoint name: {}", endpoint_name);

    Ok(())
}

/// Load OpenAPI schema from URL or local file
fn load_schema(source: &str) -> Result<String, Box<dyn std::error::Error>> {
    if source.starts_with("http://") || source.starts_with("https://") {
        // Download from URL
        Ok(reqwest::blocking::get(source)?.text()?)
    } else {
        // Read from local file
        Ok(fs::read_to_string(source)?)
    }
}

fn find_operation_by_id(
    schema: &Value,
    operation_id: &str,
) -> Result<(String, String, String), Box<dyn std::error::Error>> {
    let paths = schema.get("paths").ok_or("No paths in schema")?;

    for (path, path_obj) in paths.as_object().unwrap() {
        if let Some(path_obj) = path_obj.as_object() {
            for (method, method_obj) in path_obj {
                if let Some(method_obj) = method_obj.as_object()
                    && let Some(op_id) = method_obj.get("operationId")
                    && let Some(op_id_str) = op_id.as_str()
                    && op_id_str == operation_id
                {
                    let api_path = method_obj
                        .get("x-connector-api-path")
                        .and_then(|v| v.as_str())
                        .unwrap_or(path)
                        .to_string();
                    return Ok((path.clone(), method.clone(), api_path));
                }
            }
        }
    }

    Err(format!("Operation with ID '{}' not found in schema", operation_id).into())
}

fn print_available_endpoints(openapi_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    if openapi_url.starts_with("http://") || openapi_url.starts_with("https://") {
        println!("Fetching API description from: {}", openapi_url);
    } else {
        println!("Reading API description from: {}", openapi_url);
    }
    let spec_raw = load_schema(openapi_url)?;
    let schema_json = parse_api_spec(&spec_raw)?;

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

                    let http_path = method_obj
                        .get("x-connector-api-path")
                        .and_then(|v| v.as_str())
                        .unwrap_or(path);

                    // Check if endpoint is already implemented (as action, trigger, or both)
                    let impl_type = check_operation_implementation(op_id_str);

                    let (status_icon, name_info) = match &impl_type {
                        ImplementationType::Action(names) => {
                            ("🟢", format!("[action: {}]", names.join(", ")))
                        }
                        ImplementationType::Trigger(names) => {
                            ("🔵", format!("[trigger: {}]", names.join(", ")))
                        }
                        ImplementationType::Both {
                            action_names,
                            trigger_names,
                        } => (
                            "🟣",
                            format!(
                                "[action: {}, trigger: {}]",
                                action_names.join(", "),
                                trigger_names.join(", ")
                            ),
                        ),
                        ImplementationType::None => ("⚪", String::new()),
                    };

                    if name_info.is_empty() {
                        println!(
                            "  {} {} - {} {} - {}",
                            status_icon,
                            op_id_str,
                            method.to_uppercase(),
                            http_path,
                            description
                        );
                    } else {
                        println!(
                            "  {} {} {} - {} {} - {}",
                            status_icon,
                            op_id_str,
                            name_info,
                            method.to_uppercase(),
                            http_path,
                            description
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
enum ImplementationType {
    None,
    Action(Vec<String>),
    Trigger(Vec<String>),
    Both {
        action_names: Vec<String>,
        trigger_names: Vec<String>,
    },
}

/// Check if an operation_id is implemented by searching all action and trigger directories
/// Returns the type(s) of implementation found along with all actual names
fn check_operation_implementation(operation_id: &str) -> ImplementationType {
    let mut action_names: Vec<String> = Vec::new();
    let mut trigger_names: Vec<String> = Vec::new();

    // Search through all action directories to find all that contain this operation_id
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
                    && let Some(stored_op_id) =
                        metadata.get("operation_id").and_then(|v| v.as_str())
                    && stored_op_id == operation_id
                {
                    // Get the folder name (which is the action name)
                    if let Some(folder_name) = entry.path().file_name().and_then(|n| n.to_str()) {
                        let name = folder_name.to_string();
                        // Only add if not already in the list (avoid duplicates from default name check)
                        if !action_names.contains(&name) {
                            action_names.push(name);
                        }
                    }
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
                    && let Some(stored_op_id) =
                        metadata.get("operation_id").and_then(|v| v.as_str())
                    && stored_op_id == operation_id
                {
                    // Get the folder name (which is the trigger name)
                    if let Some(folder_name) = entry.path().file_name().and_then(|n| n.to_str()) {
                        let name = folder_name.to_string();
                        // Only add if not already in the list (avoid duplicates from default name check)
                        if !trigger_names.contains(&name) {
                            trigger_names.push(name);
                        }
                    }
                }
            }
        }
    }

    match (action_names.is_empty(), trigger_names.is_empty()) {
        (false, false) => ImplementationType::Both {
            action_names,
            trigger_names,
        },
        (false, true) => ImplementationType::Action(action_names),
        (true, false) => ImplementationType::Trigger(trigger_names),
        (true, true) => ImplementationType::None,
    }
}
