use super::error::GenerateActionError;
use serde_json::{Value, json};

/// Resolve $ref references in OpenAPI schemas
fn resolve_ref(schema: &Value, ref_path: &str) -> Option<Value> {
    // Handle OpenAPI 3.0 $ref format: #/components/schemas/Name
    if ref_path.starts_with("#/components/") {
        let parts: Vec<&str> = ref_path.split('/').collect();
        if parts.len() >= 4 {
            let component_type = parts[2]; // "schemas", "parameters", etc.
            let component_name = parts[3];

            if let Some(components) = schema.get("components")
                && let Some(component_section) = components.get(component_type)
                && let Some(component) = component_section.get(component_name)
            {
                return Some(component.clone());
            }
        }
    }
    None
}

/// Resolve a schema value, handling $ref references recursively
pub(crate) fn resolve_schema(schema: &Value, value: &Value) -> Value {
    if let Some(obj) = value.as_object() {
        if let Some(ref_str) = obj.get("$ref").and_then(|r| r.as_str())
            && let Some(resolved) = resolve_ref(schema, ref_str)
        {
            // Recursively resolve any $ref in the resolved schema
            return resolve_schema(schema, &resolved);
        }

        // Handle allOf - resolve each part
        if let Some(all_of) = obj.get("allOf").and_then(|a| a.as_array()) {
            let mut merged = serde_json::Map::new();
            for part in all_of {
                let resolved = resolve_schema(schema, part);
                if let Some(part_obj) = resolved.as_object() {
                    // Merge properties
                    if let Some(props) = part_obj.get("properties").and_then(|p| p.as_object()) {
                        if let Some(merged_props) =
                            merged.get_mut("properties").and_then(|p| p.as_object_mut())
                        {
                            for (k, v) in props {
                                merged_props.insert(k.clone(), v.clone());
                            }
                        } else {
                            merged.insert("properties".to_string(), json!(props));
                        }
                    }
                    // Merge required fields
                    if let Some(req) = part_obj.get("required").and_then(|r| r.as_array()) {
                        if let Some(merged_req) =
                            merged.get_mut("required").and_then(|r| r.as_array_mut())
                        {
                            for r in req {
                                if let Some(r_str) = r.as_str()
                                    && !merged_req.contains(&json!(r_str))
                                {
                                    merged_req.push(json!(r_str));
                                }
                            }
                        } else {
                            merged.insert("required".to_string(), json!(req));
                        }
                    }
                    // Copy other fields (type, etc.)
                    for (k, v) in part_obj {
                        if k != "properties" && k != "required" {
                            merged.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            return json!(merged);
        }
    }
    value.clone()
}

/// Recursively resolve $ref and allOf in a value, and convert enums to anyOf
pub(crate) fn resolve_refs_recursive(schema: &Value, value: &Value) -> Value {
    // First resolve $ref and allOf at this level
    let resolved = resolve_schema(schema, value);

    match resolved {
        Value::Object(obj) => {
            // Convert enum to anyOf if present
            let mut resolved_obj = obj.clone();
            convert_enum_to_anyof(&mut resolved_obj);

            // Recursively resolve all values in the object
            let mut final_obj = serde_json::Map::new();
            for (k, v) in resolved_obj {
                final_obj.insert(k.clone(), resolve_refs_recursive(schema, &v));
            }
            json!(final_obj)
        }
        Value::Array(arr) => {
            json!(
                arr.iter()
                    .map(|v| resolve_refs_recursive(schema, v))
                    .collect::<Vec<_>>()
            )
        }
        _ => resolved,
    }
}

/// Extract properties from a schema, handling $ref, allOf, and direct properties
pub(crate) fn extract_properties(
    schema: &Value,
    schema_value: &Value,
) -> serde_json::Map<String, Value> {
    let resolved = resolve_schema(schema, schema_value);
    let mut properties = serde_json::Map::new();

    if let Some(obj) = resolved.as_object() {
        // Direct properties - resolve any $ref in property values
        if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
            for (key, value) in props {
                let resolved_value = resolve_refs_recursive(schema, value);
                properties.insert(key.clone(), resolved_value);
            }
        }

        // Handle array with items
        if obj.get("type").and_then(|t| t.as_str()) == Some("array")
            && let Some(items) = obj.get("items")
        {
            let items_resolved = resolve_schema(schema, items);
            let items_fully_resolved = resolve_refs_recursive(schema, &items_resolved);
            if let Some(items_obj) = items_fully_resolved.as_object()
                && let Some(items_props) = items_obj.get("properties").and_then(|p| p.as_object())
            {
                let mut array_schema = serde_json::Map::new();
                array_schema.insert("type".to_string(), json!("array"));
                let mut item_schema = serde_json::Map::new();
                item_schema.insert("type".to_string(), json!("object"));
                item_schema.insert("properties".to_string(), json!(items_props));
                array_schema.insert("items".to_string(), json!(item_schema));
                properties.insert("items".to_string(), json!(array_schema));
            }
        }
    }

    properties
}

/// Convert enum fields to anyOf structure for better UX
fn convert_enum_to_anyof(schema: &mut serde_json::Map<String, Value>) {
    // Check if enum exists and extract values
    let enum_values = if let Some(enum_values) = schema.get("enum")
        && let Some(enum_array) = enum_values.as_array()
    {
        enum_array.clone()
    } else {
        return; // No enum to convert
    };

    // Remove the enum field
    schema.remove("enum");

    // Create anyOf structure
    let mut any_of_array = Vec::new();
    for enum_val in enum_values {
        if let Some(enum_str) = enum_val.as_str() {
            let mut any_of_item = serde_json::Map::new();
            any_of_item.insert("const".to_string(), json!(enum_str));
            any_of_array.push(json!(any_of_item));
        }
    }

    if !any_of_array.is_empty() {
        schema.insert("anyOf".to_string(), json!(any_of_array));
    }
}

/// Find operation by ID in the schema
pub fn find_operation_by_id(
    schema: &Value,
    operation_id: &str,
) -> Result<(String, String), GenerateActionError> {
    let paths = schema
        .get("paths")
        .ok_or_else(|| GenerateActionError::SchemaError("No paths in schema".to_string()))?;

    for (path, path_obj) in paths
        .as_object()
        .ok_or_else(|| GenerateActionError::SchemaError("Paths is not an object".to_string()))?
    {
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

    Err(GenerateActionError::OperationNotFound(
        operation_id.to_string(),
    ))
}

/// Generate input schema for an action
#[allow(dead_code)]
pub fn generate_input_schema(
    schema: &Value,
    path: &str,
    method: &str,
) -> Result<Value, GenerateActionError> {
    let paths = schema
        .get("paths")
        .ok_or_else(|| GenerateActionError::SchemaError("No paths in schema".to_string()))?;
    let endpoint = paths.get(path).ok_or_else(|| {
        GenerateActionError::SchemaError(format!("No endpoint found for path: {}", path))
    })?;
    let method_obj = endpoint.get(method).ok_or_else(|| {
        GenerateActionError::SchemaError(format!("No {} method found for path: {}", method, path))
    })?;

    let mut combined_schema = serde_json::Map::new();
    combined_schema.insert(
        "$schema".to_string(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    combined_schema.insert("type".to_string(), json!("object"));
    combined_schema.insert("additionalProperties".to_string(), json!(false));

    let mut all_properties = serde_json::Map::new();
    let mut all_required = Vec::new();

    // Extract parameters
    let empty_vec = vec![];
    let parameters = method_obj
        .get("parameters")
        .and_then(|p| p.as_array())
        .unwrap_or(&empty_vec);

    for param in parameters {
        // Resolve $ref if present
        let param_resolved = resolve_schema(schema, param);

        if let Some(param_obj) = param_resolved.as_object()
            && let Some(param_in) = param_obj.get("in")
            && let Some(param_in_str) = param_in.as_str()
            && (param_in_str == "path" || param_in_str == "query")
            && let Some(name) = param_obj.get("name")
            && let Some(name_str) = name.as_str()
        {
            let mut param_schema = serde_json::Map::new();

            if let Some(schema_ref) = param_obj.get("schema") {
                let schema_resolved = resolve_schema(schema, schema_ref);
                if let Some(schema_obj) = schema_resolved.as_object() {
                    for (key, value) in schema_obj {
                        param_schema.insert(key.clone(), value.clone());
                    }
                    // Convert enum to anyOf structure for better UX
                    convert_enum_to_anyof(&mut param_schema);
                }
            } else {
                // Default to string if no schema
                param_schema.insert("type".to_string(), json!("string"));
            }

            all_properties.insert(name_str.to_string(), json!(param_schema));

            if let Some(required) = param_obj.get("required")
                && let Some(required_bool) = required.as_bool()
                && required_bool
            {
                all_required.push(name_str.to_string());
            }
        }
    }

    // Extract request body schema
    if let Some(request_body) = method_obj.get("requestBody") {
        let body_resolved = resolve_schema(schema, request_body);

        if let Some(request_body_obj) = body_resolved.as_object() {
            let request_body_schema = request_body_obj
                .get("content")
                .and_then(|content| content.get("application/json"))
                .and_then(|json| json.get("schema"))
                .map(|s| resolve_schema(schema, s));

            if let Some(body_schema) = request_body_schema
                && let Some(body_obj) = body_schema.as_object()
            {
                // Handle allOf structure
                if let Some(all_of) = body_obj.get("allOf")
                    && let Some(all_of_array) = all_of.as_array()
                {
                    for schema_part in all_of_array {
                        let part_resolved = resolve_schema(schema, schema_part);
                        if let Some(part_obj) = part_resolved.as_object() {
                            // Merge properties from this part
                            if let Some(part_properties) = part_obj.get("properties")
                                && let Some(part_props) = part_properties.as_object()
                            {
                                for (key, value) in part_props {
                                    // Resolve $ref in property values
                                    let mut prop_value = resolve_refs_recursive(schema, value);
                                    if let Some(prop_obj) = prop_value.as_object_mut() {
                                        convert_enum_to_anyof(prop_obj);
                                    }
                                    all_properties.insert(key.clone(), prop_value);
                                }
                            }

                            // Merge required fields from this part
                            if let Some(part_required) = part_obj.get("required")
                                && let Some(part_req) = part_required.as_array()
                            {
                                for req in part_req {
                                    if let Some(req_str) = req.as_str() {
                                        all_required.push(req_str.to_string());
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Handle simple properties structure
                    if let Some(body_properties) = body_obj.get("properties")
                        && let Some(body_props) = body_properties.as_object()
                    {
                        for (key, value) in body_props {
                            // Resolve $ref in property values
                            let mut prop_value = resolve_refs_recursive(schema, value);
                            if let Some(prop_obj) = prop_value.as_object_mut() {
                                convert_enum_to_anyof(prop_obj);
                            }
                            all_properties.insert(key.clone(), prop_value);
                        }
                    }

                    // Merge required fields
                    if let Some(body_required) = body_obj.get("required")
                        && let Some(body_req) = body_required.as_array()
                    {
                        for req in body_req {
                            if let Some(req_str) = req.as_str() {
                                all_required.push(req_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    combined_schema.insert("properties".to_string(), json!(all_properties));
    combined_schema.insert("required".to_string(), json!(all_required));

    Ok(json!(combined_schema))
}

/// Generate output schema for an action
#[allow(dead_code)]
pub fn generate_output_schema(
    schema: &Value,
    path: &str,
    method: &str,
) -> Result<Value, GenerateActionError> {
    let paths = schema
        .get("paths")
        .ok_or_else(|| GenerateActionError::SchemaError("No paths in schema".to_string()))?;
    let endpoint = paths.get(path).ok_or_else(|| {
        GenerateActionError::SchemaError(format!("No endpoint found for path: {}", path))
    })?;
    let method_obj = endpoint.get(method).ok_or_else(|| {
        GenerateActionError::SchemaError(format!("No {} method found for path: {}", method, path))
    })?;

    // Extract response schema - try 200, 201, 202
    let response_schema = method_obj
        .get("responses")
        .and_then(|responses| {
            responses
                .get("200")
                .or_else(|| responses.get("201"))
                .or_else(|| responses.get("202"))
        })
        .and_then(|response| {
            // Resolve $ref if response is a reference
            let response_resolved = resolve_schema(schema, response);
            response_resolved
                .as_object()
                .and_then(|r| r.get("content"))
                .and_then(|content| content.get("application/json"))
                .and_then(|json| json.get("schema"))
                .map(|s| resolve_schema(schema, s))
        })
        .unwrap_or_else(|| json!({}));

    let mut output_schema = serde_json::Map::new();
    output_schema.insert(
        "$schema".to_string(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    output_schema.insert("type".to_string(), json!("object"));
    output_schema.insert("additionalProperties".to_string(), json!(false));

    // Extract properties from the resolved schema
    let properties = extract_properties(schema, &response_schema);

    output_schema.insert("properties".to_string(), json!(properties));

    Ok(json!(output_schema))
}
