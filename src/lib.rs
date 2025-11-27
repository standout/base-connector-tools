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
