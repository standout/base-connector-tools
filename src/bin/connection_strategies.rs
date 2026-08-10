use base_connector_tools::{
    connection_strategies, ConnectionDiscovery, ResolveMetadataOptions, SuggestedStrategy,
};
use std::env;
use std::process;

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_usage();
        return Ok(());
    }

    let mut issuer = None;
    let mut api_base_url = None;
    let mut strategy_filter = None;
    let mut json_output = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--issuer" => {
                issuer = Some(next_arg(&args, &mut i, "--issuer")?);
            }
            "--api-base-url" => {
                api_base_url = Some(next_arg(&args, &mut i, "--api-base-url")?);
            }
            "--json" => {
                json_output = true;
                i += 1;
            }
            value if !value.starts_with('-') && looks_like_url(value) && api_base_url.is_none() => {
                api_base_url = Some(value.to_string());
                i += 1;
            }
            value if !value.starts_with('-') && strategy_filter.is_none() => {
                strategy_filter = Some(value.to_string());
                i += 1;
            }
            flag => return Err(format!("Unknown argument: {flag}").into()),
        }
    }

    if api_base_url.is_none() && issuer.is_none() {
        return Err(
            "Missing --api-base-url <url> and/or --issuer <url> (need at least one)".into(),
        );
    }

    let display_url = api_base_url
        .clone()
        .or_else(|| issuer.clone())
        .unwrap();

    let discovery = connection_strategies(&ResolveMetadataOptions {
        api_base_url: api_base_url.clone(),
        issuer,
    })?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&discovery)?);
        return Ok(());
    }

    print_discovery(&discovery, &display_url)?;

    if let Some(strategy_id) = strategy_filter {
        let strategy = discovery
            .strategies
            .iter()
            .find(|strategy| strategy.id == strategy_id)
            .ok_or_else(|| {
                format!(
                    "Unknown strategy '{strategy_id}'. Available: {}",
                    discovery
                        .strategies
                        .iter()
                        .map(|strategy| strategy.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;

        println!();
        print_strategy_detail(strategy);
        println!();
        print_generate_command(
            api_base_url.as_deref().unwrap_or("https://www.googleapis.com"),
            &discovery.resolved_issuer,
            strategy,
        );
    }

    Ok(())
}

fn print_discovery(
    discovery: &ConnectionDiscovery,
    api_base_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Connection strategies for {api_base_url}");
    println!();

    if let Some(resource) = &discovery.protected_resource {
        println!("Protected resource (RFC 9728)");
        if let Some(value) = &resource.resource {
            println!("  resource: {value}");
        }
        if let Some(servers) = &resource.authorization_servers {
            for server in servers {
                println!("  authorization_server: {server}");
            }
        }
        if let Some(scopes) = &resource.scopes_supported {
            println!("  scopes_supported: {}", scopes.join(", "));
        }
        println!();
    } else {
        println!("Protected resource: not found (use --issuer if discovery fails)");
        println!();
    }

    let auth = &discovery.auth_server;
    println!("Authorization server: {}", discovery.resolved_issuer);
    print_optional_field("authorization_endpoint", &auth.authorization_endpoint);
    print_optional_field("token_endpoint", &auth.token_endpoint);
    print_optional_list("grant_types_supported", &auth.grant_types_supported);
    print_optional_list("scopes_supported", &auth.scopes_supported);
    print_optional_list(
        "code_challenge_methods_supported",
        &auth.code_challenge_methods_supported,
    );
    print_optional_list(
        "token_endpoint_auth_methods_supported",
        &auth.token_endpoint_auth_methods_supported,
    );
    println!();

    println!("Suggested strategies:");
    if let Some(api) = &discovery.api_base_url {
        println!("  Run with a strategy id to show a generate command, e.g.:");
        println!("    connection_strategies {api} oauth2");
    } else {
        println!("  Run with a strategy id to show a generate command, e.g.:");
        println!("    connection_strategies --issuer {} oauth2", discovery.resolved_issuer);
    }
    println!();

    for strategy in &discovery.strategies {
        print_strategy_line(strategy);
    }

    Ok(())
}

fn print_strategy_line(strategy: &SuggestedStrategy) {
    let refresh = if strategy.refresh { "refresh" } else { "no refresh" };
    let scopes = if strategy.scopes.is_empty() {
        "no scopes in metadata".to_string()
    } else {
        strategy.scopes.join(", ")
    };

    println!(
        "  🟢 {} — {} ({}, {}, scopes: {})",
        strategy.id, strategy.title, strategy.grant_type, refresh, scopes
    );
    if strategy.pkce {
        println!("      pkce");
    }
}

fn print_strategy_detail(strategy: &SuggestedStrategy) {
    println!("Strategy: {}", strategy.id);
    println!("  title: {}", strategy.title);
    println!("  type: oauth2");
    println!("  grant_type: {}", strategy.grant_type);
    println!("  refresh: {}", strategy.refresh);
    println!("  pkce: {}", strategy.pkce);
    if strategy.scopes.is_empty() {
        println!("  scopes: (none in metadata — pass --scopes to generate)");
    } else {
        println!("  scopes: {}", strategy.scopes.join(", "));
    }
}

fn print_generate_command(api_base_url: &str, issuer: &str, strategy: &SuggestedStrategy) {
    let mut command = format!(
        "generate_connection_strategy \\\n  --api-base-url {api_base_url} \\\n  --issuer {issuer} \\\n  --strategy-id {} \\\n  --strategy-title \"{}\" \\\n  --grant-type {} \\\n  --client-id-env YOUR_CLIENT_ID_ENV \\\n  --client-secret-env YOUR_CLIENT_SECRET_ENV",
        strategy.id, strategy.title, strategy.grant_type
    );

    if !strategy.scopes.is_empty() {
        command.push_str(" \\\n  --scopes ");
        command.push_str(&strategy.scopes.join(","));
    }

    command.push_str(" \\\n  -o src/connection-config.json");

    println!("Generate command:");
    println!("{command}");
    println!();
    println!("Issuer resolved as: {issuer}");
}

fn print_optional_field(label: &str, value: &Option<String>) {
    if let Some(value) = value {
        println!("  {label}: {value}");
    }
}

fn print_optional_list(label: &str, values: &Option<Vec<String>>) {
    if let Some(values) = values {
        println!("  {label}: {}", values.join(", "));
    }
}

fn next_arg(
    args: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("Missing value for {flag}").into())
        .map(|value| {
            *index += 1;
            value
        })
}

fn looks_like_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn print_usage() {
    println!(
        "connection_strategies - list OAuth metadata and suggested connection strategies\n\
         \n\
         Usage:\n\
           connection_strategies --api-base-url <url> [strategy_id] [--json]\n\
           connection_strategies --issuer <url> [strategy_id] [--json]\n\
           connection_strategies <api_base_url> [strategy_id] [--json]\n\
         \n\
         Options:\n\
           --api-base-url <url>  API base (tries /.well-known/oauth-protected-resource)\n\
           --issuer <url>        Authorization server issuer (OIDC / RFC 8414)\n\
           --json                Output full discovery as JSON\n\
         \n\
         Most SaaS APIs lack protected-resource metadata — pass --issuer for those.\n\
         \n\
         Examples:\n\
           connection_strategies --issuer https://accounts.google.com\n\
           connection_strategies --issuer https://accounts.google.com oauth2\n\
           connection_strategies --issuer https://accounts.google.com --json"
    );
}
