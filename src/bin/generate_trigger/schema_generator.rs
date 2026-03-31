use super::error::GenerateTriggerError;
use serde_json::Value;

// Reuse schema generator from actions by including it as a module
// We need to provide the error module it expects
mod action_mod {
    pub mod error {
        include!("../generate_action/error.rs");
    }
    pub mod schema_generator {
        include!("../generate_action/schema_generator.rs");
    }
}

use action_mod::error::GenerateActionError as ActionError;

/// Find operation by ID in the schema - reuse from actions
pub fn find_operation_by_id(
    schema: &Value,
    operation_id: &str,
) -> Result<(String, String, String), GenerateTriggerError> {
    action_mod::schema_generator::find_operation_by_id(schema, operation_id).map_err(|e| match e {
        ActionError::OperationNotFound(op) => GenerateTriggerError::OperationNotFound(op),
        ActionError::SchemaError(msg) => GenerateTriggerError::SchemaError(msg),
        _ => GenerateTriggerError::SchemaError(format!("{}", e)),
    })
}

/// Generate input schema for a trigger
/// Triggers typically have empty input schemas (no user input needed for polling)
pub fn generate_input_schema(
    _schema: &Value,
    _path: &str,
    _method: &str,
) -> Result<Value, GenerateTriggerError> {
    // Triggers don't need user input - they poll based on store data
    let mut input_schema = serde_json::Map::new();
    input_schema.insert(
        "$schema".to_string(),
        serde_json::json!("https://json-schema.org/draft/2020-12/schema"),
    );
    input_schema.insert("type".to_string(), serde_json::json!("object"));
    input_schema.insert("additionalProperties".to_string(), serde_json::json!(false));
    input_schema.insert("properties".to_string(), serde_json::json!({}));
    input_schema.insert("required".to_string(), serde_json::json!([]));
    Ok(serde_json::json!(input_schema))
}

/// Generate output schema for a trigger
/// For triggers, if the response is an array, the output schema should represent a single event (one item)
pub fn generate_output_schema(
    schema: &Value,
    path: &str,
    method: &str,
) -> Result<Value, GenerateTriggerError> {
    let paths = schema
        .get("paths")
        .ok_or_else(|| GenerateTriggerError::SchemaError("No paths in schema".to_string()))?;
    let endpoint = paths.get(path).ok_or_else(|| {
        GenerateTriggerError::SchemaError(format!("No endpoint found for path: {}", path))
    })?;
    let method_obj = endpoint.get(method).ok_or_else(|| {
        GenerateTriggerError::SchemaError(format!("No {} method found for path: {}", method, path))
    })?;

    let response_schema = action_mod::schema_generator::get_response_schema(schema, method_obj);

    let mut output_schema = serde_json::Map::new();
    output_schema.insert(
        "$schema".to_string(),
        serde_json::json!("https://json-schema.org/draft/2020-12/schema"),
    );
    output_schema.insert("type".to_string(), serde_json::json!("object"));

    // Check if response is an array - for triggers, we want the schema of a single item
    let resolved = action_mod::schema_generator::resolve_schema(schema, &response_schema);
    let properties = if let Some(obj) = resolved.as_object() {
        if obj.get("type").and_then(|t| t.as_str()) == Some("array") {
            // For triggers: extract the items schema (represents one event)
            if let Some(items) = obj.get("items") {
                let items_resolved = action_mod::schema_generator::resolve_schema(schema, items);
                let items_fully_resolved =
                    action_mod::schema_generator::resolve_refs_recursive(schema, &items_resolved);

                // Check if items is an object (has properties) or a primitive
                if let Some(items_obj) = items_fully_resolved.as_object() {
                    if items_obj.get("properties").is_some() {
                        // Array of objects - extract properties from the item schema
                        action_mod::schema_generator::extract_properties(
                            schema,
                            &items_fully_resolved,
                        )
                    } else {
                        // Array of primitives - wrap the item schema in a property
                        let mut props = serde_json::Map::new();
                        props.insert("value".to_string(), items_fully_resolved);
                        props
                    }
                } else {
                    // Items is a primitive value (not an object) - wrap it
                    let mut props = serde_json::Map::new();
                    props.insert("value".to_string(), items_fully_resolved);
                    props
                }
            } else {
                serde_json::Map::new()
            }
        } else {
            // Not an array - use the same logic as actions
            action_mod::schema_generator::extract_properties(schema, &response_schema)
        }
    } else {
        serde_json::Map::new()
    };

    let untyped_postman = resolved
        .as_object()
        .and_then(|o| o.get("x-connector-untyped-response"))
        .and_then(|v| v.as_bool())
        == Some(true);

    if properties.is_empty() && untyped_postman {
        output_schema.insert("additionalProperties".to_string(), serde_json::json!(true));
        if let Some(desc) = resolved
            .get("description")
            .and_then(|d| d.as_str())
            .filter(|s| !s.is_empty())
        {
            output_schema.insert("description".to_string(), serde_json::json!(desc));
        }
    } else {
        output_schema.insert("additionalProperties".to_string(), serde_json::json!(false));
    }

    output_schema.insert("properties".to_string(), serde_json::json!(properties));
    Ok(serde_json::json!(output_schema))
}
