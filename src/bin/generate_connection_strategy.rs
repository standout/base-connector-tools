use base_connector_tools::{
    build_connection_config, merge_strategy_into_config, resolve_metadata, suggest_strategies,
    GenerateConnectionConfigOptions, ResolveMetadataOptions, SuggestedStrategy,
};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::Path;
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
    let mut client_id_env = None;
    let mut client_secret_env = None;
    let mut strategy_id = "oauth2".to_string();
    let mut strategy_title = None;
    let mut scopes = Vec::new();
    let mut grant_type = "authorization_code".to_string();
    let mut post_connect_path = None;
    let mut output_path = None;
    let mut all_strategies = false;
    let mut set_default = false;
    let mut replace = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--issuer" => {
                issuer = Some(next_arg(&args, &mut i, "--issuer")?);
            }
            "--api-base-url" => {
                api_base_url = Some(next_arg(&args, &mut i, "--api-base-url")?);
            }
            "--client-id-env" => {
                client_id_env = Some(next_arg(&args, &mut i, "--client-id-env")?);
            }
            "--client-secret-env" => {
                client_secret_env = Some(next_arg(&args, &mut i, "--client-secret-env")?);
            }
            "--strategy-id" => {
                strategy_id = next_arg(&args, &mut i, "--strategy-id")?;
            }
            "--strategy-title" => {
                strategy_title = Some(next_arg(&args, &mut i, "--strategy-title")?);
            }
            "--scopes" => {
                let value = next_arg(&args, &mut i, "--scopes")?;
                scopes = value
                    .split(',')
                    .map(str::trim)
                    .filter(|scope| !scope.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "--grant-type" => {
                grant_type = next_arg(&args, &mut i, "--grant-type")?;
            }
            "--post-connect-path" => {
                post_connect_path = Some(next_arg(&args, &mut i, "--post-connect-path")?);
            }
            "-o" | "--output" => {
                output_path = Some(next_arg(&args, &mut i, "--output")?);
            }
            "--all-strategies" => {
                all_strategies = true;
                i += 1;
            }
            "--set-default" => {
                set_default = true;
                i += 1;
            }
            "--replace" => {
                replace = true;
                i += 1;
            }
            flag => return Err(format!("Unknown argument: {flag}").into()),
        }
    }

    let api_base_url = api_base_url.ok_or("Missing required --api-base-url <url>")?;
    let client_id_env = client_id_env.ok_or("Missing required --client-id-env <NAME>")?;
    let client_secret_env =
        client_secret_env.ok_or("Missing required --client-secret-env <NAME>")?;

    if issuer.is_some() {
        eprintln!("Resolving OAuth metadata...");
    } else {
        eprintln!(
            "Discovering authorization server from {api_base_url}/.well-known/oauth-protected-resource..."
        );
    }

    let resolved = resolve_metadata(&ResolveMetadataOptions {
        api_base_url: Some(api_base_url.clone()),
        issuer,
    })?;

    eprintln!("Using issuer {}", resolved.resolved_issuer);

    let strategy_specs = if all_strategies {
        let suggested =
            suggest_strategies(&resolved.auth_server, resolved.protected_resource.as_ref());
        if suggested.is_empty() {
            return Err("No strategies suggested from metadata".into());
        }
        suggested
    } else {
        vec![SuggestedStrategy {
            id: strategy_id.clone(),
            title: strategy_title
                .clone()
                .unwrap_or_else(|| "OAuth 2.0".to_string()),
            grant_type: grant_type.clone(),
            refresh: false,
            pkce: false,
            scopes: scopes.clone(),
        }]
    };

    let mut generated = Value::Null;
    for (index, spec) in strategy_specs.iter().enumerate() {
        let options = GenerateConnectionConfigOptions {
            strategy_id: spec.id.clone(),
            strategy_title: Some(spec.title.clone()),
            api_base_url: api_base_url.clone(),
            client_id_env: client_id_env.clone(),
            client_secret_env: client_secret_env.clone(),
            scopes: if scopes.is_empty() {
                spec.scopes.clone()
            } else {
                scopes.clone()
            },
            grant_type: spec.grant_type.clone(),
            // Only attach post_connect once (on the first strategy document).
            post_connect_path: if index == 0 {
                post_connect_path.clone()
            } else {
                None
            },
        };

        let piece = build_connection_config(
            &resolved.auth_server,
            &options,
            resolved.protected_resource.as_ref(),
            &resolved.resolved_issuer,
        )?;

        generated = if generated.is_null() {
            piece
        } else {
            merge_strategy_into_config(&generated, &piece, false, false)?
        };
    }

    // First strategy remains default unless --set-default (then last generated wins via merge).
    if set_default
        && let Some(last) = strategy_specs.last()
    {
        generated["default_strategy"] = Value::String(last.id.clone());
    }

    let config = if let Some(path) = &output_path {
        if !replace && Path::new(path).exists() {
            let existing: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
            let before = existing
                .get("strategies")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let merged = merge_strategy_into_config(
                &existing,
                &generated,
                set_default || before == 0,
                post_connect_path.is_some(),
            )?;
            let after = merged
                .get("strategies")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            eprintln!(
                "Merged into existing {path} ({} → {} strategies)",
                before, after
            );
            merged
        } else {
            generated
        }
    } else {
        generated
    };

    let json = serde_json::to_string_pretty(&config)?;

    if let Some(path) = output_path {
        fs::write(&path, format!("{json}\n"))?;
        eprintln!("Wrote connection-config to {path}");
    } else {
        println!("{json}");
    }

    Ok(())
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

fn print_usage() {
    println!(
        "generate_connection_strategy - build connection-config.json from OAuth discovery\n\
         \n\
         Usage:\n\
           generate_connection_strategy --api-base-url <url> \\\n\
             --client-id-env <NAME> \\\n\
             --client-secret-env <NAME> \\\n\
             [--issuer <url>] \\\n\
             [--strategy-id oauth2] \\\n\
             [--strategy-title \"OAuth 2.0\"] \\\n\
             [--scopes read,write] \\\n\
             [--grant-type authorization_code] \\\n\
             [--post-connect-path /health] \\\n\
             [--all-strategies] \\\n\
             [--set-default] \\\n\
             [--replace] \\\n\
             [-o src/connection-config.json]\n\
         \n\
         When -o points at an existing file, the new strategy is appended (same id is replaced).\n\
         Use --replace to overwrite the file instead of merging.\n\
         Use --all-strategies to generate every suggested grant (oauth2 + client_credentials).\n\
         Use --set-default to point default_strategy at the strategy being generated.\n\
         \n\
         Discovery chain (build time only):\n\
           1. GET {{api}}/.well-known/oauth-protected-resource (RFC 9728)\n\
           2. authorization_servers[0] -> issuer\n\
           3. GET {{issuer}}/.well-known/oauth-authorization-server or openid-configuration\n\
         \n\
         Example:\n\
           generate_connection_strategy \\\n\
             --api-base-url https://www.googleapis.com \\\n\
             --issuer https://accounts.google.com \\\n\
             --client-id-env GOOGLE_CLIENT_ID \\\n\
             --client-secret-env GOOGLE_CLIENT_SECRET \\\n\
             --scopes https://www.googleapis.com/auth/drive.readonly \\\n\
             -o src/connection-config.json"
    );
}
