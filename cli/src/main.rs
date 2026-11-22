//! Rust CLI migration, initially workspace registry commands only.
//! CLI beta does not touch the legacy Go runtime state or claim device pairing readiness.
use codelocal::workspace::Registry;
use serde_json::json;
use std::{env, path::Path, process::ExitCode};

const HELP: &str = r#"CodeLocal Rust beta CLI

Usage:
  codelocal [command] [options]

Available in isolated Rust beta:
  codelocal --version, -v            Print version
  codelocal --help, -h               Show this help
  codelocal .                        Authorize current directory in beta state
  codelocal <project-path>           Authorize a project directory in beta state
  codelocal grant <project-path>     Authorize a project directory in beta state
  codelocal ungrant <path|id>        Revoke beta authorization without deleting project
  codelocal workspaces               List beta-authorized workspaces
  codelocal status                   Show beta migration readiness

Not yet migrated from Go (will fail explicitly):
  codelocal login|logout|pair|switch-account
  codelocal setup|doctor|agent|approvals|mcp
  codelocal stop|reset|uninstall
  codelocal                          Start local device runtime

Environment:
  CODELOCAL_BETA_STATE_DIR           Beta-only local state directory
                                     Default: ~/.codelocal-beta

IMPORTANT: This is NOT the production CodeLocal client.
No pairing, cloud sync, MCP or privileged execution is available yet.
"#;

fn run(args: &[String]) -> Result<(), String> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "--version" | "-v" | "version" if args.len() == 1 => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--help" | "-h" | "help" if args.len() == 1 => {
            print!("{HELP}");
            Ok(())
        }
        "grant" | "add" | "." if command == "." && args.len() == 1 || (command != "." && args.len() <= 2) => {
            let path = if command == "." { "." } else { args.get(1).map(String::as_str).unwrap_or(".") };
            let registry = Registry::beta_default().map_err(|error| error.to_string())?;
            let ws = registry.grant(path).map_err(|error| error.to_string())?;
            println!("✓ Workspace added: {}\n  ID: {}\n  Path: {}", ws.workspace_name, ws.workspace_id, ws.local_path.display());
            println!("Rust beta authorization stored locally only; runtime sync is not yet implemented.");
            Ok(())
        }
        "ungrant" | "remove" if args.len() == 2 => {
            let registry = Registry::beta_default().map_err(|error| error.to_string())?;
            if registry.revoke(&args[1]).map_err(|error| error.to_string())? {
                println!("✓ Workspace authorization removed from Rust beta state.");
                Ok(())
            } else {
                Err("workspace not found".into())
            }
        }
        "workspaces" if args.len() == 1 => {
            let registry = Registry::beta_default().map_err(|error| error.to_string())?;
            let items = registry.list().map_err(|error| error.to_string())?;
            if items.is_empty() {
                println!("No local CodeLocal beta workspaces authorized.");
            } else {
                for ws in items {
                    println!("{}\t{}\t{}", ws.workspace_id, ws.workspace_name, ws.local_path.display());
                }
            }
            Ok(())
        }
        "status" if args.len() == 1 => {
            let registry = Registry::beta_default().map_err(|error| error.to_string())?;
            let items = registry.list().map_err(|error| error.to_string())?;
            let output = json!({
                "version": env!("CARGO_PKG_VERSION"),
                "channel": "rust-beta",
                "stateDir": registry.state_dir(),
                "workspaceCount": items.len(),
                "runtime": "not_implemented",
                "pairing": "not_implemented",
                "mcp": "not_implemented",
                "legacyStateModified": false
            });
            println!("{}", serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?);
            Ok(())
        }
        "" => Err("CodeLocal Rust device runtime is not implemented yet; run --help for available beta commands".into()),
        "login" | "logout" | "signout" | "pair" | "switch-account" | "switch" |
        "setup" | "doctor" | "agent" | "approvals" | "mcp" | "stop" | "reset" | "uninstall" => {
            Err(format!("{command}: not implemented in Rust beta; the production Go CLI remains authoritative"))
        }
        other if args.len() == 1 && !other.starts_with('-') && Path::new(other).is_dir() => {
            let registry = Registry::beta_default().map_err(|error| error.to_string())?;
            let ws = registry.grant(other).map_err(|error| error.to_string())?;
            println!("✓ Workspace added: {}\n  ID: {}\n  Path: {}", ws.workspace_name, ws.workspace_id, ws.local_path.display());
            println!("Rust beta authorization stored locally only; runtime sync is not yet implemented.");
            Ok(())
        }
        other => Err(format!("invalid or unavailable CodeLocal Rust beta command: {other} (run --help)")),
    }
}

fn main() -> ExitCode {
    match run(&env::args().skip(1).collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("Error: {message}");
            ExitCode::FAILURE
        }
    }
}
