//! Postman Collection v2.1 → synthetic OpenAPI 3.0 for shared tooling.
//!
//! Each request becomes a unique internal path `/_postman/{operation_id}` so duplicate
//! URL + method pairs (common in RPC-style APIs) stay addressable. The real path is
//! stored on the operation as extension field `x-connector-api-path`.

use crate::to_snake_case;
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::error::Error;
use std::fmt;

/// Detect Postman Collection JSON (v2.0+).
pub fn is_postman_collection(v: &Value) -> bool {
    if v.get("openapi").is_some() || v.get("swagger").is_some() {
        return false;
    }
    let Some(info) = v.get("info").and_then(|i| i.as_object()) else {
        return false;
    };
    let Some(items) = v.get("item").and_then(|i| i.as_array()) else {
        return false;
    };
    if items.is_empty() {
        return false;
    }
    let schema_url = info.get("schema").and_then(|s| s.as_str()).unwrap_or("");
    if schema_url.contains("postman.com") || schema_url.contains("getpostman") {
        return true;
    }
    info.contains_key("_postman_id")
}

/// Convert a Postman collection JSON value into a minimal OpenAPI 3.0 document.
pub fn postman_collection_to_openapi(collection: &Value) -> Result<Value, PostmanConversionError> {
    let items = collection
        .get("item")
        .and_then(|i| i.as_array())
        .ok_or(PostmanConversionError::MissingItemArray)?;

    let title = collection
        .get("info")
        .and_then(|i| i.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("Postman collection")
        .to_string();

    let mut operations = Vec::new();
    let mut used_ids = HashSet::new();
    walk_items(items, &mut Vec::new(), &mut operations, &mut used_ids)?;

    if operations.is_empty() {
        return Err(PostmanConversionError::NoRequests);
    }

    let mut paths = Map::new();
    for op in operations {
        let internal_path = format!("/_postman/{}", op.operation_id);
        let mut method_obj = Map::new();
        method_obj.insert("operationId".to_string(), json!(op.operation_id));
        method_obj.insert("summary".to_string(), json!(op.display_name));
        if let Some(desc) = &op.description {
            if !desc.is_empty() {
                method_obj.insert("description".to_string(), json!(desc));
            }
        }
        method_obj.insert("x-connector-api-path".to_string(), json!(op.api_path));
        method_obj.insert("parameters".to_string(), json!(op.parameters));

        if !op
            .request_body_schema
            .as_object()
            .is_some_and(|o| o.is_empty())
            || op.request_body_schema.get("type").is_some()
        {
            method_obj.insert(
                "requestBody".to_string(),
                json!({
                    "content": {
                        "application/json": {
                            "schema": op.request_body_schema
                        }
                    }
                }),
            );
        }

        method_obj.insert(
            "responses".to_string(),
            json!({
                "200": {
                    "description": "OK",
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "object",
                                "additionalProperties": true,
                                "x-connector-untyped-response": true,
                                "description": "Postman collections do not define response bodies; refine this schema from API docs or samples."
                            }
                        }
                    }
                }
            }),
        );

        let method_key = op.method.to_lowercase();
        let path_entry = paths.entry(internal_path).or_insert_with(|| json!({}));
        let path_obj = path_entry
            .as_object_mut()
            .ok_or_else(|| PostmanConversionError::InvalidStructure("path value".into()))?;
        path_obj.insert(method_key, json!(method_obj));
    }

    Ok(json!({
        "openapi": "3.0.0",
        "info": {
            "title": title,
            "version": "1.0.0"
        },
        "paths": paths
    }))
}

struct CollectedOperation {
    operation_id: String,
    display_name: String,
    method: String,
    api_path: String,
    parameters: Vec<Value>,
    request_body_schema: Value,
    description: Option<String>,
}

fn walk_items(
    items: &[Value],
    folder_stack: &mut Vec<String>,
    out: &mut Vec<CollectedOperation>,
    used_ids: &mut HashSet<String>,
) -> Result<(), PostmanConversionError> {
    for item in items {
        if let Some(sub) = item.get("item").and_then(|i| i.as_array()) {
            let name = item
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("folder")
                .to_string();
            folder_stack.push(name);
            walk_items(sub, folder_stack, out, used_ids)?;
            folder_stack.pop();
            continue;
        }

        let Some(request) = item.get("request") else {
            continue;
        };
        let request = resolve_request_object(request)?;
        let name = item
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("request")
            .to_string();
        let display_name = join_display_path(folder_stack, &name);
        let operation_id = unique_snake_id(&display_name, used_ids);

        let method = request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("GET")
            .to_lowercase();

        let (api_path, path_param_names, query_specs) = extract_url(request)?;

        let mut parameters: Vec<Value> = Vec::new();
        for pname in path_param_names {
            parameters.push(json!({
                "name": pname,
                "in": "path",
                "required": true,
                "schema": { "type": "string" }
            }));
        }
        for (qname, _qvalue, disabled) in query_specs {
            if disabled {
                continue;
            }
            // Sample query values (e.g. API keys) should not force `required` in generated schemas.
            parameters.push(json!({
                "name": qname,
                "in": "query",
                "required": false,
                "schema": { "type": "string" }
            }));
        }

        let request_body_schema = extract_body_schema(request)?;
        let description = request
            .get("description")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                item.get("description")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string())
            });

        out.push(CollectedOperation {
            operation_id,
            display_name,
            method,
            api_path,
            parameters,
            request_body_schema,
            description,
        });
    }
    Ok(())
}

fn resolve_request_object(
    request: &Value,
) -> Result<&serde_json::Map<String, Value>, PostmanConversionError> {
    if let Some(arr) = request.as_array() {
        let first = arr
            .first()
            .ok_or(PostmanConversionError::EmptyRequestArray)?;
        return resolve_request_object(first);
    }
    request
        .as_object()
        .ok_or_else(|| PostmanConversionError::InvalidRequest("request is not an object".into()))
}

fn join_display_path(folders: &[String], name: &str) -> String {
    let mut parts: Vec<&str> = folders.iter().map(|s| s.as_str()).collect();
    parts.push(name);
    parts.join(" / ")
}

fn unique_snake_id(display: &str, used: &mut HashSet<String>) -> String {
    let base = to_snake_case(display);
    let base = if base.is_empty() {
        "request".into()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2u32;
    while used.contains(&id) {
        id = format!("{base}_{n}");
        n += 1;
    }
    used.insert(id.clone());
    id
}

/// Strip query string and fragment, take path from absolute URL or use as relative path.
fn path_from_raw_url(raw: &str) -> String {
    let no_frag = raw.split('#').next().unwrap_or(raw);
    let base = no_frag.split('?').next().unwrap_or(no_frag);
    let path_part = if let Some(idx) = base.find("://") {
        let after = &base[idx + 3..];
        if let Some(slash) = after.find('/') {
            after[slash..].to_string()
        } else {
            "/".to_string()
        }
    } else if base.starts_with('/') {
        base.to_string()
    } else {
        format!("/{}", base.trim_start_matches('/'))
    };
    normalize_postman_braces(&path_part)
}

fn normalize_postman_braces(s: &str) -> String {
    // {{var}} → {var} for connector path templates
    let mut rest = s;
    let mut out = String::with_capacity(s.len());
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("}}") {
            let name = &rest[..end];
            if !name.is_empty() {
                out.push('{');
                out.push_str(name);
                out.push('}');
                rest = &rest[end + 2..];
            } else {
                out.push_str("{{");
            }
        } else {
            out.push_str("{{");
            break;
        }
    }
    out.push_str(rest);
    out
}

fn extract_url(
    request: &serde_json::Map<String, Value>,
) -> Result<(String, Vec<String>, Vec<(String, String, bool)>), PostmanConversionError> {
    let url = request
        .get("url")
        .ok_or(PostmanConversionError::MissingUrl)?;

    let (raw_hint, path_segments, query_array) = match url {
        Value::String(s) => (Some(s.as_str()), None, None),
        Value::Object(u) => {
            let raw = u.get("raw").and_then(|r| r.as_str());
            let segs = u.get("path").and_then(|p| p.as_array());
            let q = u.get("query").and_then(|q| q.as_array());
            (raw, segs, q)
        }
        _ => {
            return Err(PostmanConversionError::InvalidUrl);
        }
    };

    // Prefer `raw` when present so we keep a real path; `path` arrays often contain `{{baseUrl}}` etc.
    let mut api_path = if let Some(r) = raw_hint {
        path_from_raw_url(r)
    } else if let Some(segs) = path_segments {
        build_path_from_segments(segs)?
    } else {
        "/".to_string()
    };

    if api_path.is_empty() {
        api_path = "/".to_string();
    }

    let mut path_param_names: Vec<String> = Vec::new();
    for seg in api_path.split('/') {
        let seg = seg.trim();
        if seg.starts_with('{') && seg.ends_with('}') && seg.len() > 2 {
            path_param_names.push(seg[1..seg.len() - 1].to_string());
        }
    }

    let mut query_specs: Vec<(String, String, bool)> = Vec::new();
    if let Some(qarr) = query_array {
        for q in qarr {
            let Some(qo) = q.as_object() else {
                continue;
            };
            let disabled = qo
                .get("disabled")
                .and_then(|d| d.as_bool())
                .unwrap_or(false);
            let key = qo.get("key").and_then(|k| k.as_str()).unwrap_or("");
            if key.is_empty() {
                continue;
            }
            let val = qo
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            query_specs.push((key.to_string(), val, disabled));
        }
    }

    if let Some(r) = raw_hint {
        if let Some((_base, qs)) = r.split_once('?') {
            for pair in qs.split('&') {
                let pair = pair.split('#').next().unwrap_or(pair);
                if pair.is_empty() {
                    continue;
                }
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                let k = k.trim();
                if k.is_empty() {
                    continue;
                }
                let v_decoded = v.trim().to_string();
                if query_specs.iter().any(|(qk, _, _)| qk == k) {
                    continue;
                }
                query_specs.push((k.to_string(), v_decoded, false));
            }
        }
    }

    Ok((api_path, path_param_names, query_specs))
}

fn build_path_from_segments(segs: &[Value]) -> Result<String, PostmanConversionError> {
    let mut parts = Vec::new();
    for s in segs {
        let piece = match s {
            Value::String(t) => t.clone(),
            Value::Object(o) => o
                .get("value")
                .or_else(|| o.get("key"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            _ => String::new(),
        };
        if piece.is_empty() {
            continue;
        }
        let seg = if let Some(stripped) = piece.strip_prefix(':') {
            format!("{{{stripped}}}")
        } else {
            normalize_postman_braces(&piece)
        };
        parts.push(seg);
    }
    if parts.is_empty() {
        return Ok("/".to_string());
    }
    Ok(format!("/{}", parts.join("/")))
}

/// Postman stores `body.raw` as a string or as `{ "language": "json", "value": "..." }`.
fn postman_raw_body_text(body: &serde_json::Map<String, Value>) -> Option<String> {
    let raw = body.get("raw")?;
    match raw {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o
            .get("value")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// Replace `{{ ... }}` with JSON `null` when not inside a string, so template bodies parse.
fn replace_postman_template_vars_with_null_outside_strings(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut idx = 0usize;
    let mut in_string = false;
    let mut escape = false;

    while idx < input.len() {
        let ch = input[idx..].chars().next().unwrap();
        let len = ch.len_utf8();

        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            idx += len;
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            idx += len;
            continue;
        }

        let b = input.as_bytes();
        if idx + 1 < b.len() && b[idx] == b'{' && b[idx + 1] == b'{' {
            if let Some(rel_end) = input[idx + 2..].find("}}") {
                out.push_str("null");
                idx += 2 + rel_end + 2;
                continue;
            }
        }

        out.push(ch);
        idx += len;
    }
    out
}

fn parse_postman_raw_as_json(raw: &str) -> Option<Value> {
    serde_json::from_str(raw).ok().or_else(|| {
        let sanitized = replace_postman_template_vars_with_null_outside_strings(raw);
        serde_json::from_str(&sanitized).ok()
    })
}

/// Build OpenAPI JSON Schema for `requestBody` from a parsed Postman raw JSON value.
fn schema_for_inferred_body_value(v: &Value) -> Value {
    match v {
        Value::Object(map) if map.is_empty() => json!({}),
        Value::Object(_) => infer_json_schema(v),
        Value::Array(_) | Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {
            let mut sch = infer_json_schema(v);
            if let Some(obj) = sch.as_object_mut() {
                obj.insert("x-connector-body-field-name".to_string(), json!("body"));
            }
            sch
        }
    }
}

fn extract_body_schema(
    request: &serde_json::Map<String, Value>,
) -> Result<Value, PostmanConversionError> {
    let Some(body) = request.get("body").and_then(|b| b.as_object()) else {
        return Ok(json!({}));
    };
    let mode = body.get("mode").and_then(|m| m.as_str()).unwrap_or("raw");
    match mode {
        "raw" => {
            let Some(text) = postman_raw_body_text(body) else {
                return Ok(json!({}));
            };
            let raw = text.trim();
            if raw.is_empty() {
                return Ok(json!({}));
            }
            if let Some(v) = parse_postman_raw_as_json(raw) {
                return Ok(schema_for_inferred_body_value(&v));
            }
            Ok(json!({
                "type": "string",
                "description": "Raw request body (not valid JSON in collection)",
                "x-connector-body-field-name": "body"
            }))
        }
        "urlencoded" => {
            let Some(params) = body.get("urlencoded").and_then(|p| p.as_array()) else {
                return Ok(json!({}));
            };
            let mut props = Map::new();
            let mut required = Vec::new();
            for p in params {
                let Some(po) = p.as_object() else {
                    continue;
                };
                if po
                    .get("disabled")
                    .and_then(|d| d.as_bool())
                    .unwrap_or(false)
                {
                    continue;
                }
                let key = po.get("key").and_then(|k| k.as_str()).unwrap_or("");
                if key.is_empty() {
                    continue;
                }
                props.insert(key.to_string(), json!({ "type": "string" }));
                if po
                    .get("value")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty())
                {
                    required.push(json!(key));
                }
            }
            Ok(json!({
                "type": "object",
                "properties": props,
                "required": required
            }))
        }
        "formdata" => Ok(json!({
            "type": "object",
            "additionalProperties": true,
            "description": "multipart/form-data body (inspect collection for fields)"
        })),
        _ => Ok(json!({})),
    }
}

fn infer_json_schema(v: &Value) -> Value {
    match v {
        Value::Null => json!({ "type": "null" }),
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Number(n) => {
            if n.is_i64() {
                json!({ "type": "integer" })
            } else {
                json!({ "type": "number" })
            }
        }
        Value::String(_) => json!({ "type": "string" }),
        Value::Array(arr) => {
            let items = arr
                .first()
                .map(|x| infer_json_schema(x))
                .unwrap_or_else(|| json!({ "type": "object", "additionalProperties": true }));
            json!({
                "type": "array",
                "items": items
            })
        }
        Value::Object(map) => {
            let mut props = Map::new();
            for (k, val) in map {
                props.insert(k.clone(), infer_json_schema(val));
            }
            json!({
                "type": "object",
                "properties": props,
                "required": []
            })
        }
    }
}

#[derive(Debug)]
pub enum PostmanConversionError {
    MissingItemArray,
    NoRequests,
    MissingUrl,
    InvalidUrl,
    InvalidRequest(String),
    EmptyRequestArray,
    InvalidStructure(String),
}

impl fmt::Display for PostmanConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PostmanConversionError::MissingItemArray => {
                write!(f, "Postman collection has no item array")
            }
            PostmanConversionError::NoRequests => {
                write!(f, "Postman collection contains no HTTP requests")
            }
            PostmanConversionError::MissingUrl => write!(f, "Postman request has no url"),
            PostmanConversionError::InvalidUrl => {
                write!(f, "Postman request url has invalid shape")
            }
            PostmanConversionError::InvalidRequest(s) => write!(f, "Postman request: {s}"),
            PostmanConversionError::EmptyRequestArray => {
                write!(f, "Postman request array is empty")
            }
            PostmanConversionError::InvalidStructure(s) => {
                write!(f, "Invalid Postman structure: {s}")
            }
        }
    }
}

impl Error for PostmanConversionError {}

/// Parse YAML or JSON API description: OpenAPI / Swagger, or Postman Collection.
pub fn parse_api_spec(content: &str) -> Result<Value, ApiSpecError> {
    let trimmed = content.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        let v: Value = serde_json::from_str(content).map_err(ApiSpecError::Json)?;
        if is_postman_collection(&v) {
            return postman_collection_to_openapi(&v).map_err(ApiSpecError::Postman);
        }
        if v.get("openapi").is_some() || v.get("swagger").is_some() {
            return Ok(v);
        }
        return Err(ApiSpecError::UnrecognizedJson);
    }

    let yaml_val: serde_yaml::Value = serde_yaml::from_str(content).map_err(ApiSpecError::Yaml)?;
    let v = serde_json::to_value(yaml_val).map_err(ApiSpecError::Json)?;
    if is_postman_collection(&v) {
        return postman_collection_to_openapi(&v).map_err(ApiSpecError::Postman);
    }
    Ok(v)
}

#[derive(Debug)]
pub enum ApiSpecError {
    Json(serde_json::Error),
    Yaml(serde_yaml::Error),
    Postman(PostmanConversionError),
    UnrecognizedJson,
}

impl fmt::Display for ApiSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiSpecError::Json(e) => write!(f, "JSON: {e}"),
            ApiSpecError::Yaml(e) => write!(f, "YAML: {e}"),
            ApiSpecError::Postman(e) => write!(f, "{e}"),
            ApiSpecError::UnrecognizedJson => write!(
                f,
                "JSON is neither OpenAPI/Swagger nor a Postman collection (missing openapi/swagger/info+item)"
            ),
        }
    }
}

impl Error for ApiSpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ApiSpecError::Json(e) => Some(e),
            ApiSpecError::Yaml(e) => Some(e),
            ApiSpecError::Postman(e) => Some(e),
            ApiSpecError::UnrecognizedJson => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_minimal_collection() {
        let col = json!({
            "info": {
                "name": "Test",
                "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
            },
            "item": [
                {
                    "name": "List Users",
                    "request": {
                        "method": "GET",
                        "url": "https://api.example.com/v1/users?limit=10"
                    }
                }
            ]
        });
        let oas = postman_collection_to_openapi(&col).unwrap();
        let paths = oas.get("paths").unwrap().as_object().unwrap();
        assert_eq!(paths.len(), 1);
        let (_path, path_obj) = paths.iter().next().unwrap();
        let get = path_obj.get("get").unwrap();
        assert_eq!(
            get.get("operationId").and_then(|x| x.as_str()),
            Some("list_users")
        );
        assert_eq!(
            get.get("x-connector-api-path").and_then(|x| x.as_str()),
            Some("/v1/users")
        );
    }

    #[test]
    fn postman_braces_normalized() {
        assert_eq!(normalize_postman_braces("/v1/{{id}}/x"), "/v1/{id}/x");
    }

    #[test]
    fn template_vars_replaced_outside_strings() {
        let s = r#"{"key": {{apiKey}}, "name": "a {{b}} c"}"#;
        let out = replace_postman_template_vars_with_null_outside_strings(s);
        assert!(serde_json::from_str::<Value>(&out).is_ok());
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v.get("key").unwrap().is_null());
        assert_eq!(v.get("name").and_then(|x| x.as_str()), Some("a {{b}} c"));
    }

    #[test]
    fn raw_body_object_with_value_field() {
        let body = json!({
            "mode": "raw",
            "raw": { "language": "json", "value": "{\"foo\": 1, \"bar\": {{x}} }" }
        });
        let req = json!({ "method": "POST", "url": "https://example.com/x", "body": body });
        let m = req.as_object().unwrap();
        let sch = extract_body_schema(m).unwrap();
        let props = sch.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("foo"));
        assert!(props.contains_key("bar"));
    }
}
