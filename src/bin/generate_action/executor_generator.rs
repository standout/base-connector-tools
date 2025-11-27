use super::error::GenerateActionError;
use super::template_manager::TemplateManager;
use super::utils::{
    extract_path_parameter_names, generate_path_parameter_extraction,
    generate_query_parameter_building, to_snake_case,
};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Generate executor code for an action
pub fn generate_executor_code(
    schema: &Value,
    action_name: &str,
    schema_path: &str,
    method: &str,
    api_path: &str,
) -> Result<String, GenerateActionError> {
    let paths = schema
        .get("paths")
        .ok_or_else(|| GenerateActionError::SchemaError("No paths in schema".to_string()))?;
    let endpoint = paths.get(schema_path).ok_or_else(|| {
        GenerateActionError::SchemaError(format!("No endpoint found for path: {}", schema_path))
    })?;
    let method_obj = endpoint.get(method).ok_or_else(|| {
        GenerateActionError::SchemaError(format!(
            "No {} method found for path: {}",
            method, schema_path
        ))
    })?;

    // Extract parameters and resolve $ref references
    let empty_vec = vec![];
    let parameters_raw = method_obj
        .get("parameters")
        .and_then(|p| p.as_array())
        .unwrap_or(&empty_vec);

    // Resolve $ref references in parameters
    let mut parameters = Vec::new();
    for param in parameters_raw {
        if let Some(param_obj) = param.as_object() {
            if let Some(ref_str) = param_obj.get("$ref").and_then(|r| r.as_str()) {
                // Resolve the $ref
                if let Some(resolved) = resolve_parameter_ref(schema, ref_str) {
                    parameters.push(resolved);
                } else {
                    // If resolution fails, keep the original
                    parameters.push(param.clone());
                }
            } else {
                parameters.push(param.clone());
            }
        } else {
            parameters.push(param.clone());
        }
    }

    // Extract request body schema
    let request_body_schema = method_obj
        .get("requestBody")
        .and_then(|rb| rb.get("content"))
        .and_then(|content| content.get("application/json"))
        .and_then(|json| json.get("schema"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    // Extract path parameter names for the is_path_parameter function
    let path_param_names = extract_path_parameter_names(&parameters);
    let has_path_params = !path_param_names.contains("__none__");

    // Generate request body handling
    let body_handling = generate_body_handling(&request_body_schema, &path_param_names)?;

    // Generate HTTP method call using the API path
    let (http_call, input_data_used) = generate_http_call(method, api_path, &parameters);

    // Generate different code based on HTTP method
    let executor_code = if method.to_lowercase() == "get" {
        generate_get_executor_code(action_name, &http_call, &parameters, input_data_used)?
    } else if method.to_lowercase() == "delete" {
        generate_delete_executor_code(
            action_name,
            &http_call,
            &path_param_names,
            has_path_params,
            &parameters,
        )?
    } else {
        generate_post_patch_executor_code(
            action_name,
            &body_handling,
            &http_call,
            &path_param_names,
            has_path_params,
            &parameters,
        )?
    };

    Ok(executor_code)
}

/// Generate GET executor code
fn generate_get_executor_code(
    action_name: &str,
    http_call: &str,
    parameters: &[Value],
    input_data_used: bool,
) -> Result<String, GenerateActionError> {
    let has_path_params = !extract_path_parameter_names(parameters).contains("__none__");

    // Check if there are query parameters
    let has_query_params = parameters.iter().any(|param| {
        if let Some(param_obj) = param.as_object()
            && let Some(param_in) = param_obj.get("in")
            && let Some(param_in_str) = param_in.as_str()
            && param_in_str == "query"
            && param_obj.get("name").is_some()
        {
            true
        } else {
            false
        }
    });

    // Initialize template manager and load templates
    let mut template_manager = TemplateManager::new();
    template_manager.load_standard_templates()?;

    // Generate path parameter functions if needed
    let path_parameter_functions = if has_path_params {
        let mut variables = HashMap::new();
        variables.insert(
            "PATH_PARAMETER_EXTRACTION".to_string(),
            generate_path_parameter_extraction(parameters),
        );
        template_manager.render("path_parameter_functions", &variables)?
    } else {
        String::new()
    };

    // Generate query parameter functions if needed
    let query_parameter_functions = if has_query_params {
        let mut variables = HashMap::new();
        variables.insert(
            "QUERY_PARAMETER_BUILDING".to_string(),
            generate_query_parameter_building(parameters),
        );
        template_manager.render("query_parameter_functions", &variables)?
    } else {
        String::new()
    };

    // Render the main template
    let mut variables = HashMap::new();
    variables.insert("HTTP_CALL".to_string(), http_call.to_string());
    variables.insert(
        "ACTION_NAME".to_string(),
        determine_custom_field_type_from_action_name(action_name),
    );
    variables.insert(
        "PATH_PARAMETER_FUNCTIONS".to_string(),
        path_parameter_functions,
    );
    variables.insert(
        "QUERY_PARAMETER_FUNCTIONS".to_string(),
        query_parameter_functions,
    );
    variables.insert(
        "INPUT_DATA_PARAM".to_string(),
        if input_data_used {
            "input_data".to_string()
        } else {
            "_input_data".to_string()
        },
    );

    template_manager.render("get_executor", &variables)
}

/// Generate DELETE executor code
fn generate_delete_executor_code(
    action_name: &str,
    http_call: &str,
    _path_param_names: &str,
    has_path_params: bool,
    parameters: &[Value],
) -> Result<String, GenerateActionError> {
    // Initialize template manager and load templates
    let mut template_manager = TemplateManager::new();
    template_manager.load_standard_templates()?;

    // Generate path parameter functions if needed
    let path_parameter_functions = if has_path_params {
        let mut variables = HashMap::new();
        variables.insert(
            "PATH_PARAMETER_EXTRACTION".to_string(),
            generate_path_parameter_extraction(parameters),
        );
        template_manager.render("path_parameter_functions", &variables)?
    } else {
        String::new()
    };

    // Render the main template
    let mut variables = HashMap::new();
    variables.insert("HTTP_CALL".to_string(), http_call.to_string());
    variables.insert(
        "ACTION_NAME".to_string(),
        determine_custom_field_type_from_action_name(action_name),
    );
    variables.insert(
        "PATH_PARAMETER_FUNCTIONS".to_string(),
        path_parameter_functions,
    );

    template_manager.render("delete_executor", &variables)
}

/// Generate POST/PATCH executor code
fn generate_post_patch_executor_code(
    action_name: &str,
    body_handling: &str,
    http_call: &str,
    path_param_names: &str,
    has_path_params: bool,
    parameters: &[Value],
) -> Result<String, GenerateActionError> {
    // Initialize template manager and load templates
    let mut template_manager = TemplateManager::new();
    template_manager.load_standard_templates()?;

    // Generate path parameter functions if needed
    let path_parameter_functions = if has_path_params {
        let mut variables = HashMap::new();
        variables.insert(
            "PATH_PARAMETER_EXTRACTION".to_string(),
            generate_path_parameter_extraction(parameters),
        );
        template_manager.render("path_parameter_functions", &variables)?
    } else {
        String::new()
    };

    // Render the main template
    let mut variables = HashMap::new();
    variables.insert("BODY_HANDLING".to_string(), body_handling.to_string());
    variables.insert("HTTP_CALL".to_string(), http_call.to_string());
    variables.insert("PATH_PARAM_NAMES".to_string(), path_param_names.to_string());
    variables.insert(
        "ACTION_NAME".to_string(),
        determine_custom_field_type_from_action_name(action_name),
    );
    variables.insert(
        "PATH_PARAMETER_FUNCTIONS".to_string(),
        path_parameter_functions,
    );

    template_manager.render("post_patch_executor", &variables)
}

/// Generate body handling code
fn generate_body_handling(
    request_body_schema: &Value,
    path_param_names: &str,
) -> Result<String, GenerateActionError> {
    if request_body_schema.is_object()
        && !request_body_schema
            .as_object()
            .is_some_and(|obj| obj.is_empty())
    {
        // Parse path parameter names from the string format
        let path_params = if path_param_names == "\"__none__\"" {
            "&[]"
        } else {
            // Convert from "param1", "param2" format to ["param1", "param2"]
            let params: Vec<&str> = path_param_names
                .split(',')
                .map(|s| s.trim().trim_matches('"'))
                .collect();
            &format!("&{:?}", params)
        };

        Ok(format!(
            r#"    let request_body = request_body_without_empty_values(input_data, {})?;
"#,
            path_params
        ))
    } else {
        Ok(
            r#"    let request_body = serde_json::Value::Null; // No request body required
"#
            .to_string(),
        )
    }
}

/// Generate HTTP call code and return whether input_data is used
fn generate_http_call(method: &str, api_path: &str, parameters: &[Value]) -> (String, bool) {
    let http_method = method.to_uppercase();
    let url_path = api_path;
    let has_path_params = url_path.contains('{');

    // Check if there are query parameters
    let has_query_params = parameters.iter().any(|param| {
        if let Some(param_obj) = param.as_object()
            && let Some(param_in) = param_obj.get("in")
            && let Some(param_in_str) = param_in.as_str()
            && param_in_str == "query"
        {
            true
        } else {
            false
        }
    });

    match http_method.as_str() {
        "GET" => {
            let input_data_used = has_path_params || has_query_params;

            if has_path_params && has_query_params {
                (
                    format!(
                        r#"    let endpoint = build_endpoint("{}", &extract_path_parameters(input_data)?);
    let query_params = build_query_parameters(input_data)?;
    let full_endpoint = if query_params.is_empty() {{
        endpoint
    }} else {{
        format!("{{}}?{{}}", endpoint, query_params)
    }};
    let response = client.get(&full_endpoint)?;

    Ok(response)
"#,
                        url_path
                    ),
                    input_data_used,
                )
            } else if has_path_params {
                (
                    format!(
                        r#"    let endpoint = build_endpoint("{}", &extract_path_parameters(input_data)?);
    let response = client.get(&endpoint)?;

    Ok(response)
"#,
                        url_path
                    ),
                    input_data_used,
                )
            } else if has_query_params {
                (
                    format!(
                        r#"    let endpoint = "{}".to_string();
    let query_params = build_query_parameters(input_data)?;
    let full_endpoint = if query_params.is_empty() {{
        endpoint
    }} else {{
        format!("{{}}?{{}}", endpoint, query_params)
    }};
    let response = client.get(&full_endpoint)?;

    Ok(response)
"#,
                        url_path
                    ),
                    input_data_used,
                )
            } else {
                (
                    format!(
                        r#"    let endpoint = "{}".to_string();
    let response = client.get(&endpoint)?;

    Ok(response)
"#,
                        url_path
                    ),
                    input_data_used,
                )
            }
        }
        "POST" => {
            if has_path_params {
                (
                    format!(
                        r#"    let endpoint = build_endpoint("{}", &extract_path_parameters(input_data)?);
    let response = client.post(&endpoint, &request_body).map_err(|e| AppError {{
        code: ErrorCode::Other,
        message: format!("POST request failed - URL: API base URL{{}}, Body: {{}}, Error: {{}}",
                        endpoint, serde_json::to_string(&request_body).unwrap_or_else(|_| "Failed to serialize".to_string()), e.message),
    }})?;

    Ok(response)
"#,
                        url_path
                    ),
                    true, // input_data is used for both path params and request body
                )
            } else {
                (
                    format!(
                        r#"    let endpoint = "{}";
    let response = client.post(endpoint, &request_body).map_err(|e| AppError {{
        code: ErrorCode::Other,
        message: format!("POST request failed - URL: API base URL{{}}, Body: {{}}, Error: {{}}",
                        endpoint, serde_json::to_string(&request_body).unwrap_or_else(|_| "Failed to serialize".to_string()), e.message),
    }})?;

    Ok(response)
"#,
                        url_path
                    ),
                    true, // input_data is used for request body
                )
            }
        }
        "PATCH" => {
            if has_path_params {
                (
                    format!(
                        r#"    let endpoint = build_endpoint("{}", &extract_path_parameters(input_data)?);
    let response = client.patch(&endpoint, &request_body).map_err(|e| AppError {{
        code: ErrorCode::Other,
        message: format!("PATCH request failed - URL: API base URL{{}}, Body: {{}}, Error: {{}}",
                        endpoint, serde_json::to_string(&request_body).unwrap_or_else(|_| "Failed to serialize".to_string()), e.message),
    }})?;

    Ok(response)
"#,
                        url_path
                    ),
                    true, // input_data is used for both path params and request body
                )
            } else {
                (
                    format!(
                        r#"    let endpoint = "{}";
    let response = client.patch(endpoint, &request_body).map_err(|e| AppError {{
        code: ErrorCode::Other,
        message: format!("PATCH request failed - URL: API base URL{{}}, Body: {{}}, Error: {{}}",
                        endpoint, serde_json::to_string(&request_body).unwrap_or_else(|_| "Failed to serialize".to_string()), e.message),
    }})?;

    Ok(response)
"#,
                        url_path
                    ),
                    true, // input_data is used for request body
                )
            }
        }
        "PUT" => {
            if has_path_params {
                (
                    format!(
                        r#"    let endpoint = build_endpoint("{}", &extract_path_parameters(input_data)?);
    let response = client.put(&endpoint, &request_body).map_err(|e| AppError {{
        code: ErrorCode::Other,
        message: format!("PUT request failed - URL: API base URL{{}}, Body: {{}}, Error: {{}}",
                        endpoint, serde_json::to_string(&request_body).unwrap_or_else(|_| "Failed to serialize".to_string()), e.message),
    }})?;

    Ok(response)
"#,
                        url_path
                    ),
                    true, // input_data is used for both path params and request body
                )
            } else {
                (
                    format!(
                        r#"    let endpoint = "{}";
    let response = client.put(endpoint, &request_body).map_err(|e| AppError {{
        code: ErrorCode::Other,
        message: format!("PUT request failed - URL: API base URL{{}}, Body: {{}}, Error: {{}}",
                        endpoint, serde_json::to_string(&request_body).unwrap_or_else(|_| "Failed to serialize".to_string()), e.message),
    }})?;

    Ok(response)
"#,
                        url_path
                    ),
                    true, // input_data is used for request body
                )
            }
        }
        "DELETE" => {
            if has_path_params {
                (
                    format!(
                        r#"    let endpoint = build_endpoint("{}", &extract_path_parameters(input_data)?);
    let response = client.delete(&endpoint)?;

    Ok(response)
"#,
                        url_path
                    ),
                    true, // input_data is used for path params
                )
            } else {
                (
                    format!(
                        r#"    let endpoint = "{}".to_string();
    let response = client.delete(&endpoint)?;

    Ok(response)
"#,
                        url_path
                    ),
                    false, // input_data is not used
                )
            }
        }
        _ => {
            (
                format!(
                    r#"    return Err(AppError {{
        code: ErrorCode::Other,
        message: format!("Unsupported HTTP method: {{}}", "{}"),
    }});
"#,
                    http_method
                ),
                false, // input_data is not used
            )
        }
    }
}

/// Resolve a $ref reference for a parameter
fn resolve_parameter_ref(schema: &Value, ref_path: &str) -> Option<Value> {
    // Handle OpenAPI 3.0 $ref format: #/components/parameters/Name
    if ref_path.starts_with("#/components/") {
        let parts: Vec<&str> = ref_path.split('/').collect();
        if parts.len() >= 4 {
            let component_type = parts[2]; // "parameters", "schemas", etc.
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

/// Determine the custom field type from action name
/// Examples:
/// - "getDealProducts" -> "product" (sub-entity)
/// - "addOrganization" -> "organization" (main entity)
/// - "deleteDealProduct" -> "product" (sub-entity)
/// - "getPersonFollowers" -> "follower" (sub-entity)
fn determine_custom_field_type_from_action_name(action_name: &str) -> String {
    // Convert camelCase to snake_case first
    let snake_case = to_snake_case(action_name);

    // Split by underscore
    let parts: Vec<&str> = snake_case.split('_').collect();

    let last_part = parts.last().unwrap();

    if last_part.ends_with("s") && last_part.len() > 3 {
        // products -> product
        last_part[..last_part.len() - 1].to_string()
    } else {
        last_part.to_string()
    }
}
