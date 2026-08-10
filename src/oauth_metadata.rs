//! OAuth discovery (RFC 8414, OIDC, RFC 9728) and connection-config generation.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::error::Error;
use std::fmt;

const AUTH_SERVER_WELL_KNOWN_SUFFIXES: &[&str] = &[
    "/.well-known/oauth-authorization-server",
    "/.well-known/openid-configuration",
];

const PROTECTED_RESOURCE_WELL_KNOWN: &str = "/.well-known/oauth-protected-resource";

#[derive(Debug)]
pub struct OAuthMetadataError(String);

impl fmt::Display for OAuthMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for OAuthMetadataError {}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OAuthAuthorizationServerMetadata {
    pub issuer: Option<String>,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub revocation_endpoint: Option<String>,
    pub introspection_endpoint: Option<String>,
    pub registration_endpoint: Option<String>,
    pub scopes_supported: Option<Vec<String>>,
    pub response_types_supported: Option<Vec<String>>,
    pub grant_types_supported: Option<Vec<String>>,
    pub code_challenge_methods_supported: Option<Vec<String>>,
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OAuthProtectedResourceMetadata {
    pub resource: Option<String>,
    pub authorization_servers: Option<Vec<String>>,
    pub scopes_supported: Option<Vec<String>>,
    pub bearer_methods_supported: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ResolveMetadataOptions {
    pub api_base_url: Option<String>,
    pub issuer: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedMetadata {
    pub auth_server: OAuthAuthorizationServerMetadata,
    pub protected_resource: Option<OAuthProtectedResourceMetadata>,
    pub resolved_issuer: String,
}

#[derive(Debug, Clone)]
pub struct GenerateConnectionConfigOptions {
    pub strategy_id: String,
    pub strategy_title: Option<String>,
    pub api_base_url: String,
    pub client_id_env: String,
    pub client_secret_env: String,
    pub scopes: Vec<String>,
    pub grant_type: String,
    pub post_connect_path: Option<String>,
}

fn http_client() -> Result<reqwest::blocking::Client, OAuthMetadataError> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| OAuthMetadataError(e.to_string()))
}

fn get_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<T, OAuthMetadataError> {
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| OAuthMetadataError(format!("GET {url}: {e}")))?;

    if !response.status().is_success() {
        return Err(OAuthMetadataError(format!(
            "GET {url} returned {}",
            response.status()
        )));
    }

    response
        .json()
        .map_err(|e| OAuthMetadataError(format!("Invalid metadata JSON from {url}: {e}")))
}

/// Fetch RFC 9728 protected resource metadata for an API base URL.
pub fn fetch_protected_resource_metadata(
    resource_url: &str,
) -> Result<OAuthProtectedResourceMetadata, OAuthMetadataError> {
    let base = resource_url.trim_end_matches('/');
    let url = format!("{base}{PROTECTED_RESOURCE_WELL_KNOWN}");
    get_json(&http_client()?, &url)
}

/// Fetch RFC 8414 / OIDC authorization server metadata for an issuer URL.
pub fn fetch_metadata(issuer: &str) -> Result<OAuthAuthorizationServerMetadata, OAuthMetadataError> {
    let issuer = issuer.trim_end_matches('/');
    let client = http_client()?;

    for suffix in AUTH_SERVER_WELL_KNOWN_SUFFIXES {
        let url = format!("{issuer}{suffix}");
        if let Ok(metadata) = get_json::<OAuthAuthorizationServerMetadata>(&client, &url) {
            return Ok(metadata);
        }
    }

    Err(OAuthMetadataError(format!(
        "Could not fetch OAuth metadata for issuer {issuer}"
    )))
}

/// Resolve auth server metadata from API protected-resource metadata and/or issuer.
pub fn resolve_metadata(
    options: &ResolveMetadataOptions,
) -> Result<ResolvedMetadata, OAuthMetadataError> {
    let mut protected_resource = None;
    let mut issuer = options.issuer.clone();

    if let Some(api_base_url) = &options.api_base_url {
        if let Ok(metadata) = fetch_protected_resource_metadata(api_base_url) {
            if issuer.is_none() {
                issuer = metadata
                    .authorization_servers
                    .as_ref()
                    .and_then(|servers| servers.first().cloned());
            }
            protected_resource = Some(metadata);
        }
    }

    let issuer = issuer.ok_or_else(|| {
        OAuthMetadataError(
            "Could not determine issuer. Provide --issuer or an API URL with \
             /.well-known/oauth-protected-resource"
                .into(),
        )
    })?;

    let auth_server = fetch_metadata(&issuer)?;

    Ok(ResolvedMetadata {
        auth_server,
        protected_resource,
        resolved_issuer: issuer.trim_end_matches('/').to_string(),
    })
}

fn refresh_supported(metadata: &OAuthAuthorizationServerMetadata, grant_type: &str) -> bool {
    if grant_type == "client_credentials" {
        return false;
    }

    metadata
        .grant_types_supported
        .as_ref()
        .map(|grants| grants.iter().any(|grant| grant == "refresh_token"))
        .unwrap_or(true)
}

fn pkce_required(metadata: &OAuthAuthorizationServerMetadata) -> bool {
    metadata
        .code_challenge_methods_supported
        .as_ref()
        .map(|methods| methods.iter().any(|method| method == "S256"))
        .unwrap_or(false)
}

fn resolve_scopes(
    options: &GenerateConnectionConfigOptions,
    metadata: &OAuthAuthorizationServerMetadata,
    protected_resource: Option<&OAuthProtectedResourceMetadata>,
) -> Vec<String> {
    if !options.scopes.is_empty() {
        return options.scopes.clone();
    }

    resolve_scopes_from_metadata(metadata, protected_resource)
}

fn resolve_scopes_from_metadata(
    metadata: &OAuthAuthorizationServerMetadata,
    protected_resource: Option<&OAuthProtectedResourceMetadata>,
) -> Vec<String> {
    if let Some(resource) = protected_resource {
        if let Some(scopes) = &resource.scopes_supported {
            if !scopes.is_empty() {
                return scopes.clone();
            }
        }
    }

    metadata.scopes_supported.clone().unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
pub struct SuggestedStrategy {
    pub id: String,
    pub title: String,
    pub grant_type: String,
    pub refresh: bool,
    pub pkce: bool,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionDiscovery {
    pub api_base_url: Option<String>,
    pub resolved_issuer: String,
    pub protected_resource: Option<OAuthProtectedResourceMetadata>,
    pub auth_server: OAuthAuthorizationServerMetadata,
    pub strategies: Vec<SuggestedStrategy>,
}

/// Suggest connection strategies from resolved OAuth metadata.
pub fn suggest_strategies(
    metadata: &OAuthAuthorizationServerMetadata,
    protected_resource: Option<&OAuthProtectedResourceMetadata>,
) -> Vec<SuggestedStrategy> {
    let grants = metadata
        .grant_types_supported
        .clone()
        .unwrap_or_else(|| vec!["authorization_code".to_string()]);

    let scopes = resolve_scopes_from_metadata(metadata, protected_resource);
    let mut strategies = Vec::new();

    if grants.iter().any(|grant| grant == "authorization_code") {
        strategies.push(SuggestedStrategy {
            id: "oauth2".into(),
            title: "OAuth 2.0".into(),
            grant_type: "authorization_code".into(),
            refresh: refresh_supported(metadata, "authorization_code"),
            pkce: pkce_required(metadata),
            scopes: scopes.clone(),
        });
    }

    if grants.iter().any(|grant| grant == "client_credentials") {
        strategies.push(SuggestedStrategy {
            id: "client_credentials".into(),
            title: "Client credentials".into(),
            grant_type: "client_credentials".into(),
            refresh: false,
            pkce: false,
            scopes: scopes.clone(),
        });
    }

    if strategies.is_empty() {
        strategies.push(SuggestedStrategy {
            id: "oauth2".into(),
            title: "OAuth 2.0".into(),
            grant_type: "authorization_code".into(),
            refresh: refresh_supported(metadata, "authorization_code"),
            pkce: pkce_required(metadata),
            scopes,
        });
    }

    strategies
}

/// Discover OAuth metadata and suggest connection strategies (no config file written).
pub fn connection_strategies(
    options: &ResolveMetadataOptions,
) -> Result<ConnectionDiscovery, OAuthMetadataError> {
    let resolved = resolve_metadata(options)?;

    let strategies = suggest_strategies(&resolved.auth_server, resolved.protected_resource.as_ref());

    Ok(ConnectionDiscovery {
        api_base_url: options.api_base_url.clone(),
        resolved_issuer: resolved.resolved_issuer,
        protected_resource: resolved.protected_resource,
        auth_server: resolved.auth_server,
        strategies,
    })
}

fn default_storage(refresh: bool) -> Vec<&'static str> {
    if refresh {
        vec![
            "access_token",
            "refresh_token",
            "expires_at",
            "strategy_id",
        ]
    } else {
        vec!["access_token", "expires_at", "strategy_id"]
    }
}

fn authorization_server_from_metadata(
    metadata: &OAuthAuthorizationServerMetadata,
    resolved_issuer: &str,
) -> Result<Value, OAuthMetadataError> {
    let token_endpoint = metadata
        .token_endpoint
        .clone()
        .ok_or_else(|| OAuthMetadataError("metadata missing token_endpoint".into()))?;

    let issuer = metadata
        .issuer
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| resolved_issuer.to_string());

    let mut authorization_server = json!({
        "issuer": issuer.trim_end_matches('/'),
        "token_endpoint": token_endpoint
    });

    if let Some(value) = &metadata.authorization_endpoint {
        authorization_server["authorization_endpoint"] = json!(value);
    }
    if let Some(value) = &metadata.response_types_supported {
        authorization_server["response_types_supported"] = json!(value);
    }
    if let Some(value) = &metadata.grant_types_supported {
        authorization_server["grant_types_supported"] = json!(value);
    }
    if let Some(value) = &metadata.scopes_supported {
        authorization_server["scopes_supported"] = json!(value);
    }
    if let Some(value) = &metadata.code_challenge_methods_supported {
        authorization_server["code_challenge_methods_supported"] = json!(value);
    }
    if let Some(value) = &metadata.token_endpoint_auth_methods_supported {
        authorization_server["token_endpoint_auth_methods_supported"] = json!(value);
    }

    Ok(authorization_server)
}

fn protected_resource_from_metadata(
    api_base_url: &str,
    resolved_issuer: &str,
    metadata: &OAuthAuthorizationServerMetadata,
    protected_resource: Option<&OAuthProtectedResourceMetadata>,
) -> Value {
    let mut resource = json!({
        "resource": api_base_url,
        "authorization_servers": [resolved_issuer.trim_end_matches('/')]
    });

    if let Some(protected) = protected_resource {
        if let Some(value) = &protected.resource {
            resource["resource"] = json!(value);
        }
        if let Some(value) = &protected.authorization_servers {
            resource["authorization_servers"] = json!(value);
        }
        if let Some(value) = &protected.scopes_supported {
            resource["scopes_supported"] = json!(value);
        }
        if let Some(value) = &protected.bearer_methods_supported {
            resource["bearer_methods_supported"] = json!(value);
        }
    } else if let Some(scopes) = &metadata.scopes_supported {
        if !scopes.is_empty() {
            resource["scopes_supported"] = json!(scopes);
        }
    }

    resource
}

/// Build a single oauth2 strategy object from resolved metadata.
pub fn build_strategy(
    metadata: &OAuthAuthorizationServerMetadata,
    options: &GenerateConnectionConfigOptions,
    protected_resource: Option<&OAuthProtectedResourceMetadata>,
    resolved_issuer: &str,
) -> Result<Value, OAuthMetadataError> {
    if options.grant_type == "authorization_code"
        && metadata.authorization_endpoint.is_none()
    {
        return Err(OAuthMetadataError(
            "metadata missing authorization_endpoint for authorization_code".into(),
        ));
    }

    let scopes = resolve_scopes(options, metadata, protected_resource);
    let refresh = refresh_supported(metadata, &options.grant_type);

    let authorization_server =
        authorization_server_from_metadata(metadata, resolved_issuer)?;

    let mut oauth_client = json!({
        "grant_type": options.grant_type,
        "client_id": format!("{{{{env:{}}}}}", options.client_id_env),
        "client_secret": format!("{{{{env:{}}}}}", options.client_secret_env)
    });

    if !scopes.is_empty() {
        oauth_client["scopes"] = json!(scopes);
    }

    // Platform reads oauth_client.pkce; otherwise it infers from
    // authorization_server.code_challenge_methods_supported.
    if options.grant_type == "authorization_code" && pkce_required(metadata) {
        oauth_client["pkce"] = json!(true);
    }

    let mut strategy = json!({
        "id": options.strategy_id,
        "type": "oauth2",
        "title": options.strategy_title.as_deref().unwrap_or("OAuth 2.0"),
        "authorization_server": authorization_server,
        "oauth_client": oauth_client,
        "storage": default_storage(refresh)
    });

    if !refresh {
        strategy["refresh"] = json!({ "enabled": false });
    }

    Ok(strategy)
}

/// Build a connection-config JSON document from resolved metadata.
pub fn build_connection_config(
    metadata: &OAuthAuthorizationServerMetadata,
    options: &GenerateConnectionConfigOptions,
    protected_resource: Option<&OAuthProtectedResourceMetadata>,
    resolved_issuer: &str,
) -> Result<Value, OAuthMetadataError> {
    let strategy = build_strategy(metadata, options, protected_resource, resolved_issuer)?;

    let mut config = json!({
        "strategies": [strategy],
        "default_strategy": options.strategy_id,
        "protected_resource": protected_resource_from_metadata(
            &options.api_base_url,
            resolved_issuer,
            metadata,
            protected_resource
        ),
        "runtime": {
            "headers": {
                "Authorization": { "template": "Bearer {{access_token}}" },
                "Content-Type": "application/json",
                "Accept": "application/json"
            }
        }
    });

    if let Some(path) = &options.post_connect_path {
        config["post_connect"] = json!({
            "request": { "method": "GET", "path": path },
            "success_status": [200]
        });
    }

    Ok(config)
}

/// Upsert a strategy into an existing connection-config document.
///
/// - Replaces a strategy with the same `id`, otherwise appends.
/// - Leaves `default_strategy` alone unless empty or `set_default` is true.
/// - Fills missing `protected_resource` / `runtime` from `incoming`.
/// - Sets `post_connect` from `incoming` only when missing, or when `replace_post_connect`.
pub fn merge_strategy_into_config(
    existing: &Value,
    incoming: &Value,
    set_default: bool,
    replace_post_connect: bool,
) -> Result<Value, OAuthMetadataError> {
    let mut config = if existing.is_object() {
        existing.clone()
    } else {
        return Err(OAuthMetadataError(
            "existing connection-config must be a JSON object".into(),
        ));
    };

    let incoming_strategies = incoming
        .get("strategies")
        .and_then(|v| v.as_array())
        .ok_or_else(|| OAuthMetadataError("incoming config missing strategies".into()))?;

    let strategies = config
        .as_object_mut()
        .ok_or_else(|| OAuthMetadataError("existing connection-config must be an object".into()))?
        .entry("strategies")
        .or_insert_with(|| json!([]));

    let strategies_arr = strategies.as_array_mut().ok_or_else(|| {
        OAuthMetadataError("existing connection-config strategies must be an array".into())
    })?;

    for strategy in incoming_strategies {
        let id = strategy
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OAuthMetadataError("strategy missing id".into()))?;

        if let Some(index) = strategies_arr.iter().position(|existing_strategy| {
            existing_strategy.get("id").and_then(|v| v.as_str()) == Some(id)
        }) {
            strategies_arr[index] = strategy.clone();
        } else {
            strategies_arr.push(strategy.clone());
        }
    }

    let default_empty = config
        .get("default_strategy")
        .and_then(|v| v.as_str())
        .map(|s| s.is_empty())
        .unwrap_or(true);

    if set_default || default_empty {
        if let Some(default) = incoming.get("default_strategy") {
            config["default_strategy"] = default.clone();
        }
    }

    if config.get("protected_resource").is_none() {
        if let Some(resource) = incoming.get("protected_resource") {
            config["protected_resource"] = resource.clone();
        }
    }

    if config.get("runtime").is_none()
        || config
            .get("runtime")
            .and_then(|v| v.as_object())
            .is_some_and(|o| o.is_empty())
    {
        if let Some(runtime) = incoming.get("runtime") {
            config["runtime"] = runtime.clone();
        }
    }

    if replace_post_connect || config.get("post_connect").is_none() {
        if let Some(post_connect) = incoming.get("post_connect") {
            config["post_connect"] = post_connect.clone();
        }
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metadata() -> OAuthAuthorizationServerMetadata {
        OAuthAuthorizationServerMetadata {
            issuer: Some("https://auth.example.com".into()),
            authorization_endpoint: Some("https://auth.example.com/oauth/authorize".into()),
            token_endpoint: Some("https://auth.example.com/oauth/token".into()),
            revocation_endpoint: None,
            introspection_endpoint: None,
            registration_endpoint: None,
            scopes_supported: Some(vec!["read".into(), "write".into()]),
            response_types_supported: Some(vec!["code".into()]),
            grant_types_supported: Some(vec![
                "authorization_code".into(),
                "refresh_token".into(),
            ]),
            code_challenge_methods_supported: Some(vec!["S256".into()]),
            token_endpoint_auth_methods_supported: Some(vec!["client_secret_basic".into()]),
        }
    }

    #[test]
    fn build_connection_config_uses_rfc_blocks() {
        let config = build_connection_config(
            &sample_metadata(),
            &GenerateConnectionConfigOptions {
                strategy_id: "oauth2".into(),
                strategy_title: None,
                api_base_url: "https://api.example.com".into(),
                client_id_env: "MY_CLIENT_ID".into(),
                client_secret_env: "MY_CLIENT_SECRET".into(),
                scopes: vec![],
                grant_type: "authorization_code".into(),
                post_connect_path: Some("/health".into()),
            },
            None,
            "https://auth.example.com",
        )
        .unwrap();

        let auth_server = &config["strategies"][0]["authorization_server"];
        assert_eq!(auth_server["issuer"], "https://auth.example.com");
        assert_eq!(
            auth_server["authorization_endpoint"],
            "https://auth.example.com/oauth/authorize"
        );
        assert_eq!(
            auth_server["code_challenge_methods_supported"],
            json!(["S256"])
        );
        assert!(config["strategies"][0].get("oauth").is_none());

        let oauth_client = &config["strategies"][0]["oauth_client"];
        assert_eq!(oauth_client["client_id"], "{{env:MY_CLIENT_ID}}");
        assert_eq!(oauth_client["scopes"], json!(["read", "write"]));
        assert_eq!(oauth_client["pkce"], true);
        assert_eq!(config["protected_resource"]["resource"], "https://api.example.com");
        assert!(config["runtime"].get("base_url").is_none());
        assert!(config["strategies"][0].get("token_response").is_none());
        assert!(config["strategies"][0].get("refresh").is_none());
        assert_eq!(
            config["post_connect"]["request"]["path"],
            "/health"
        );
    }

    #[test]
    fn build_connection_config_disables_refresh_when_unsupported() {
        let mut metadata = sample_metadata();
        metadata.grant_types_supported = Some(vec!["authorization_code".into()]);

        let config = build_connection_config(
            &metadata,
            &GenerateConnectionConfigOptions {
                strategy_id: "oauth2".into(),
                strategy_title: None,
                api_base_url: "https://api.example.com".into(),
                client_id_env: "ID".into(),
                client_secret_env: "SECRET".into(),
                scopes: vec![],
                grant_type: "authorization_code".into(),
                post_connect_path: None,
            },
            None,
            "https://auth.example.com",
        )
        .unwrap();

        assert_eq!(config["strategies"][0]["refresh"]["enabled"], false);
    }

    #[test]
    fn resolve_scopes_prefers_protected_resource() {
        let metadata = sample_metadata();
        let protected = OAuthProtectedResourceMetadata {
            resource: Some("https://api.example.com".into()),
            authorization_servers: Some(vec!["https://auth.example.com".into()]),
            scopes_supported: Some(vec!["api.read".into()]),
            bearer_methods_supported: Some(vec!["header".into()]),
        };

        let scopes = resolve_scopes(
            &GenerateConnectionConfigOptions {
                strategy_id: "oauth2".into(),
                strategy_title: None,
                api_base_url: "https://api.example.com".into(),
                client_id_env: "ID".into(),
                client_secret_env: "SECRET".into(),
                scopes: vec![],
                grant_type: "authorization_code".into(),
                post_connect_path: None,
            },
            &metadata,
            Some(&protected),
        );

        assert_eq!(scopes, vec!["api.read"]);
    }

    #[test]
    fn suggest_strategies_from_grant_types() {
        let metadata = sample_metadata();
        let strategies = suggest_strategies(&metadata, None);

        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].id, "oauth2");
        assert!(strategies[0].refresh);
        assert!(strategies[0].pkce);
    }

    #[test]
    fn suggest_strategies_includes_client_credentials() {
        let mut metadata = sample_metadata();
        metadata
            .grant_types_supported
            .get_or_insert_default()
            .push("client_credentials".into());

        let strategies = suggest_strategies(&metadata, None);

        assert_eq!(strategies.len(), 2);
        assert_eq!(strategies[1].id, "client_credentials");
    }

    #[test]
    fn merge_appends_and_replaces_by_strategy_id() {
        let existing = json!({
            "strategies": [{
                "id": "oauth2",
                "type": "oauth2",
                "title": "Old"
            }],
            "default_strategy": "oauth2",
            "runtime": { "headers": { "Accept": "application/json" } }
        });

        let incoming = json!({
            "strategies": [
                { "id": "oauth2", "type": "oauth2", "title": "Updated" },
                { "id": "client_credentials", "type": "oauth2", "title": "CC" }
            ],
            "default_strategy": "client_credentials",
            "runtime": { "headers": { "Authorization": "Bearer x" } }
        });

        let merged = merge_strategy_into_config(&existing, &incoming, false, false).unwrap();
        let strategies = merged["strategies"].as_array().unwrap();
        assert_eq!(strategies.len(), 2);
        assert_eq!(strategies[0]["title"], "Updated");
        assert_eq!(strategies[1]["id"], "client_credentials");
        assert_eq!(merged["default_strategy"], "oauth2");
        assert_eq!(merged["runtime"]["headers"]["Accept"], "application/json");

        let merged_default =
            merge_strategy_into_config(&existing, &incoming, true, false).unwrap();
        assert_eq!(merged_default["default_strategy"], "client_credentials");
    }
}
