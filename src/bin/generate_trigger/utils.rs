// Reuse to_snake_case from the shared library
pub use base_connector_tools::to_snake_case;

/// Extract path parameter names from parameters
pub fn extract_path_parameter_names(parameters: &[serde_json::Value]) -> String {
    let path_params: Vec<String> = parameters
        .iter()
        .filter_map(|param| {
            if let Some(param_obj) = param.as_object()
                && let Some(param_in) = param_obj.get("in")
                && let Some(param_in_str) = param_in.as_str()
                && param_in_str == "path"
                && let Some(name) = param_obj.get("name")
                && let Some(name_str) = name.as_str()
            {
                return Some(name_str.to_string());
            }
            None
        })
        .collect();
    if path_params.is_empty() {
        "\"__none__\"".to_string()
    } else {
        path_params
            .iter()
            .map(|p| format!("\"{}\"", p))
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// Generate path parameter extraction code for triggers
/// Uses store_data instead of input_data
pub fn generate_path_parameter_extraction(parameters: &[serde_json::Value]) -> String {
    let mut extraction_code = String::new();
    for param in parameters {
        if let Some(param_obj) = param.as_object()
            && let Some(param_in) = param_obj.get("in")
            && let Some(param_in_str) = param_in.as_str()
            && param_in_str == "path"
            && let Some(name) = param_obj.get("name")
            && let Some(name_str) = name.as_str()
        {
            extraction_code.push_str(&format!(
                r#"    let {} = store_data.get("{}")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| store_data.get("{}")
            .and_then(|v| v.as_i64())
            .map(|i| i.to_string()))
        .ok_or_else(|| AppError {{
            code: ErrorCode::Misconfigured,
            message: "{} parameter is required in store".to_string(),
        }})?;
    params.insert("{}".to_string(), serde_json::Value::String({}));
"#,
                name_str, name_str, name_str, name_str, name_str, name_str
            ));
        }
    }

    extraction_code
}

/// Generate query parameter building code for triggers
/// Uses store_data instead of input_data
pub fn generate_query_parameter_building(parameters: &[serde_json::Value]) -> String {
    let mut building_code = String::new();
    let query_params: Vec<&str> = parameters
        .iter()
        .filter_map(|param| {
            if let Some(param_obj) = param.as_object()
                && let Some(param_in) = param_obj.get("in")
                && let Some(param_in_str) = param_in.as_str()
                && param_in_str == "query"
                && let Some(name) = param_obj.get("name")
                && let Some(name_str) = name.as_str()
            {
                Some(name_str)
            } else {
                None
            }
        })
        .collect();
    if query_params.is_empty() {
        return String::new();
    }
    for param_name in query_params {
        building_code.push_str(&format!(
            r#"    add_query_parameter(store_data, "{}", &mut query_parts);
"#,
            param_name
        ));
    }
    building_code
}
