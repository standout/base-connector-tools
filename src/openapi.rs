use crate::to_snake_case;
use serde_json::Value;

const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options", "trace"];

/// Reference to one HTTP operation in an OpenAPI paths object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRef {
    pub path: String,
    pub method: String,
    pub operation_id: Option<String>,
}

/// Error when resolving an operation by identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindOperationError {
    InvalidSchema(String),
    NotFound {
        identifier: String,
        hints: Vec<String>,
    },
    Ambiguous {
        identifier: String,
        candidates: Vec<String>,
    },
}

impl std::fmt::Display for FindOperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FindOperationError::InvalidSchema(msg) => write!(f, "{}", msg),
            FindOperationError::NotFound { identifier, hints } => {
                write!(f, "Operation '{}' not found in schema", identifier)?;
                if !hints.is_empty() {
                    write!(f, ". Try one of:")?;
                    for hint in hints {
                        write!(f, "\n  - {}", hint)?;
                    }
                }
                Ok(())
            }
            FindOperationError::Ambiguous { identifier, candidates } => {
                write!(
                    f,
                    "Operation '{}' matches multiple endpoints. Use a more specific identifier:",
                    identifier
                )?;
                for c in candidates {
                    write!(f, "\n  - {}", c)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for FindOperationError {}

/// Iterate HTTP operations under `paths`.
pub fn iter_operations(schema: &Value) -> Result<Vec<OperationRef>, FindOperationError> {
    let paths = schema
        .get("paths")
        .ok_or_else(|| FindOperationError::InvalidSchema("No paths in schema".to_string()))?;
    let paths_obj = paths.as_object().ok_or_else(|| {
        FindOperationError::InvalidSchema("Paths is not an object".to_string())
    })?;

    let mut ops = Vec::new();
    for (path, path_obj) in paths_obj {
        let Some(path_obj) = path_obj.as_object() else {
            continue;
        };
        for (method, method_obj) in path_obj {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }
            let Some(method_obj) = method_obj.as_object() else {
                continue;
            };
            ops.push(OperationRef {
                path: path.clone(),
                method: method.clone(),
                operation_id: method_obj
                    .get("operationId")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            });
        }
    }
    Ok(ops)
}

/// Build a stable operation identifier from HTTP method and path (e.g. `get /users/{id}` → `get_users_id`).
pub fn synthetic_operation_id(method: &str, path: &str) -> String {
    to_snake_case(&format!("{method} {path}"))
}

/// Identifier shown in `endpoints` and accepted by `find_operation_by_identifier`.
///
/// Uses `operationId` when present, otherwise [`synthetic_operation_id`].
pub fn operation_lookup_key(operation: &OperationRef) -> String {
    operation
        .operation_id
        .clone()
        .unwrap_or_else(|| synthetic_operation_id(&operation.method, &operation.path))
}

/// Find an operation by `operationId` or synthetic id from method + path.
pub fn find_operation_by_identifier(
    schema: &Value,
    identifier: &str,
) -> Result<(String, String), FindOperationError> {
    let operations = iter_operations(schema)?;

    let mut matches: Vec<&OperationRef> = operations
        .iter()
        .filter(|op| operation_lookup_key(op) == identifier)
        .collect();

    if matches.len() == 1 {
        let op = matches[0];
        return Ok((op.path.clone(), op.method.clone()));
    }

    if matches.is_empty() {
        let hints: Vec<String> = operations
            .iter()
            .take(8)
            .map(operation_lookup_key)
            .collect();
        return Err(FindOperationError::NotFound {
            identifier: identifier.to_string(),
            hints,
        });
    }

    matches.sort_by(|a, b| a.path.cmp(&b.path).then(a.method.cmp(&b.method)));

    let candidates: Vec<String> = matches.iter().map(|op| operation_lookup_key(op)).collect();

    Err(FindOperationError::Ambiguous {
        identifier: identifier.to_string(),
        candidates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_by_operation_id() {
        let spec = json!({
            "paths": {
                "/users": {
                    "get": { "operationId": "getUsers", "tags": ["Users"] }
                }
            }
        });
        let (path, method) = find_operation_by_identifier(&spec, "getUsers").unwrap();
        assert_eq!(path, "/users");
        assert_eq!(method, "get");
    }

    #[test]
    fn finds_by_synthetic_id_when_no_operation_id() {
        let spec = json!({
            "paths": {
                "/items": {
                    "get": { "summary": "List items" }
                }
            }
        });
        let (path, method) = find_operation_by_identifier(&spec, "get_items").unwrap();
        assert_eq!(path, "/items");
        assert_eq!(method, "get");
    }

    #[test]
    fn synthetic_id_from_path_with_params() {
        assert_eq!(
            synthetic_operation_id("get", "/accountbalances/{date}"),
            "get_accountbalances_date"
        );
    }

    #[test]
    fn lookup_key_uses_synthetic_id_without_operation_id() {
        let ops = iter_operations(&json!({
            "paths": { "/x": { "get": {} } }
        }))
        .unwrap();
        assert_eq!(operation_lookup_key(&ops[0]), "get_x");
    }

    #[test]
    fn duplicate_operation_id_is_ambiguous() {
        let spec = json!({
            "paths": {
                "/a": { "get": { "operationId": "same" } },
                "/b": { "get": { "operationId": "same" } }
            }
        });
        let err = find_operation_by_identifier(&spec, "same").unwrap_err();
        assert!(matches!(err, FindOperationError::Ambiguous { .. }));
    }
}
