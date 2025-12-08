use super::error::GenerateTriggerError;
use super::template_manager::TemplateManager;
use super::utils::{
    extract_path_parameter_names, generate_path_parameter_extraction,
    generate_query_parameter_building,
};
use serde_json::Value;
use std::collections::HashMap;

/// Generate executor code for a trigger
pub fn generate_executor_code(
    schema: &Value,
    trigger_name: &str,
    schema_path: &str,
    method: &str,
    api_path: &str,
) -> Result<String, GenerateTriggerError> {
    let paths = schema
        .get("paths")
        .ok_or_else(|| GenerateTriggerError::SchemaError("No paths in schema".to_string()))?;
    let endpoint = paths.get(schema_path).ok_or_else(|| {
        GenerateTriggerError::SchemaError(format!("No endpoint found for path: {}", schema_path))
    })?;
    let method_obj = endpoint.get(method).ok_or_else(|| {
        GenerateTriggerError::SchemaError(format!(
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
                if let Some(resolved) = resolve_parameter_ref(schema, ref_str) {
                    parameters.push(resolved);
                } else {
                    parameters.push(param.clone());
                }
            } else {
                parameters.push(param.clone());
            }
        } else {
            parameters.push(param.clone());
        }
    }

    // Extract path parameter names
    let path_param_names = extract_path_parameter_names(&parameters);
    let has_path_params = !path_param_names.contains("__none__");

    // Generate HTTP call code
    let http_call = generate_http_call(method, api_path, &parameters);

    // Generate trigger executor code (triggers typically use GET)
    let executor_code = generate_trigger_executor_code(
        trigger_name,
        &http_call,
        &path_param_names,
        has_path_params,
        &parameters,
    )?;

    Ok(executor_code)
}

/// Generate trigger executor code
fn generate_trigger_executor_code(
    _trigger_name: &str,
    http_call: &str,
    _path_param_names: &str,
    has_path_params: bool,
    parameters: &[Value],
) -> Result<String, GenerateTriggerError> {
    let mut template_manager = TemplateManager::new();
    template_manager.load_standard_templates()?;

    // Path parameter extraction is embedded in the trigger template
    let path_parameter_functions = if has_path_params {
        generate_path_parameter_extraction(parameters)
    } else {
        String::new()
    };

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

    // Query parameter building is embedded in the trigger template
    let query_parameter_functions = if has_query_params {
        generate_query_parameter_building(parameters)
    } else {
        String::new()
    };

    // Render the main template
    let mut variables = HashMap::new();
    variables.insert("HTTP_CALL".to_string(), http_call.to_string());
    variables.insert(
        "PATH_PARAMETER_EXTRACTION".to_string(),
        path_parameter_functions,
    );
    variables.insert(
        "QUERY_PARAMETER_BUILDING".to_string(),
        query_parameter_functions,
    );

    template_manager.render("trigger_executor", &variables)
}

/// Generate HTTP call code for triggers
fn generate_http_call(method: &str, api_path: &str, parameters: &[Value]) -> String {
    let http_method = method.to_uppercase();
    let url_path = api_path;
    let has_path_params = url_path.contains('{');
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
            if has_path_params && has_query_params {
                format!(
                    r#"    let endpoint = build_endpoint("{}", &extract_path_parameters(&store_data)?);
    let query_params = build_query_parameters(&store_data)?;
    let full_endpoint = if query_params.is_empty() {{
        endpoint
    }} else {{
        format!("{{}}?{{}}", endpoint, query_params)
    }};
"#,
                    url_path
                )
            } else if has_path_params {
                format!(
                    r#"    let endpoint = build_endpoint("{}", &extract_path_parameters(&store_data)?);
    let full_endpoint = endpoint;
"#,
                    url_path
                )
            } else if has_query_params {
                format!(
                    r#"    let endpoint = "{}".to_string();
    let query_params = build_query_parameters(&store_data)?;
    let full_endpoint = if query_params.is_empty() {{
        endpoint
    }} else {{
        format!("{{}}?{{}}", endpoint, query_params)
    }};
"#,
                    url_path
                )
            } else {
                format!(
                    r#"    let endpoint = "{}".to_string();
    let full_endpoint = endpoint;
"#,
                    url_path
                )
            }
        }
        _ => {
            format!(
                r#"    return Err(AppError {{
        code: ErrorCode::Other,
        message: format!("Unsupported HTTP method for trigger: {{}}", "{}"),
    }});
"#,
                http_method
            )
        }
    }
}

/// Resolve a $ref reference for a parameter
fn resolve_parameter_ref(schema: &Value, ref_path: &str) -> Option<Value> {
    if ref_path.starts_with("#/components/") {
        let parts: Vec<&str> = ref_path.split('/').collect();
        if parts.len() >= 4 {
            let component_type = parts[2];
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
