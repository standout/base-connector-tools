/// Convert a string to snake_case, handling all non-alphanumeric characters
///
/// This function converts strings to snake_case format, replacing:
/// - All non-alphanumeric characters with underscores
/// - Uppercase letters (with preceding underscore if needed)
/// - Collapses consecutive separators into a single underscore
/// - Removes leading/trailing underscores
///
/// Examples:
/// - "getUsers" -> "get_users"
/// - "get/Users" -> "get_users"
/// - "get.Users" -> "get_users"
/// - "get-Users" -> "get_users"
/// - "get//Users" -> "get_users"
pub fn to_snake_case(input: &str) -> String {
    let mut result = String::new();

    for c in input.chars() {
        if !c.is_alphanumeric() {
            // Replace all non-alphanumeric characters with underscores
            // Avoid consecutive underscores
            if result.is_empty() || !result.ends_with('_') {
                result.push('_');
            }
        } else if c.is_uppercase() && !result.is_empty() {
            // Add underscore before uppercase letters (unless at start or after underscore)
            if !result.ends_with('_') {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap_or(c));
        } else {
            result.push(c.to_lowercase().next().unwrap_or(c));
        }
    }

    // Remove leading/trailing underscores
    result.trim_matches('_').to_string()
}

/// Generate schema exports matching the WIT bundled with the target connector.
pub fn schema_functions(kind: &str, wit: &str) -> String {
    let v6 = wit
        .lines()
        .any(|line| line.trim().starts_with("package standout:app@6."));
    let prefix = if kind == "Action" { "base_" } else { "" };
    let template = if v6 {
        include_str!("../build_templates/schema_v6.rs.template")
    } else {
        include_str!("../build_templates/schema_legacy.rs.template")
    };
    ["input", "output"]
        .map(|direction| {
            template
                .replace("{KIND}", kind)
                .replace("{DIRECTION}", direction)
                .replace("{PREFIX}", prefix)
        })
        .join("\n")
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn schema_exports_match_the_target_connector() {
        for (kind, prefix) in [("Action", "base_"), ("Trigger", "")] {
            let old = schema_functions(kind, "package standout:app@5.0.0;");
            let new = schema_functions(kind, "package standout:app@6.0.0;");
            for direction in ["input", "output"] {
                for code in [&old, &new] {
                    assert!(code.contains(&format!("{prefix}{direction}_schema.json")));
                }
            }
            assert!(old.contains("Result<serde_json::Value"));
            assert!(new.contains("SchemaResponse::Complete"));
        }
    }
}
