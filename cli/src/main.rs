//! Rust CLI migration, initially workspace registry commands only.
//! CLI beta does not touch the legacy Go runtime state or claim device pairing readiness.
use codelocal::{localfs::LocalReader, workspace::Registry};
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
  codelocal login|pair               Start beta device pairing via browser approval
  codelocal login --force            Re-pair and replace saved beta credential
  codelocal sync                     Sync signed workspace metadata to beta cloud
  codelocal logout                   Signed beta credential revoke + local sign-out
  codelocal local-info <id> <file>   Inspect local file metadata inside a granted workspace
  codelocal local-read <id> <file>   Read bounded local text (no network)
  codelocal local-read <id> <file> <from> <to>   Read a bounded line range
  codelocal local-read-many <id> <file...>      Read up to 20 local files
  codelocal local-search-files <id> <query> [limit]   Find names with .gitignore
  codelocal local-search-text <id> <text> [limit]    Find fixed text in files
  codelocal local-edit <id> <file> <sha256> <old> <new> [--all]   Hash-safe local edit

Not yet migrated from Go (will fail explicitly):
  codelocal switch-account
  codelocal setup|doctor|agent|approvals|mcp
  codelocal stop|reset|uninstall
  codelocal                          Start local device runtime

Environment:
  CODELOCAL_BETA_API_URL             Beta API (HTTPS, or localhost HTTP)
  CODELOCAL_BETA_STATE_DIR           Beta-only local state directory
                                     Default: ~/.codelocal-beta

IMPORTANT: This is NOT the production CodeLocal client.
Signed manual workspace metadata sync is available. WebSocket runtime, MCP and privileged remote execution are NOT available yet.
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
            println!("Beta grant saved locally. Run `codelocal sync` to sync metadata; remote runtime is not yet implemented.");
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
                "pairing": if codelocal::pair::local_status()? { "paired_locally" } else { "not_paired" },
                "mcp": "not_implemented",
                "localRead": "granted_only_manual",
                "workspaceSync": "manual_signed_metadata",
                "legacyStateModified": false
            });
            println!("{}", serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?);
            Ok(())
        }
        "local-info" if args.len() == 3 => {
            let registry = Registry::beta_default().map_err(|err| err.to_string())?;
            let reader = LocalReader::from_registry(&registry, &args[1]).map_err(|err| err.to_string())?;
            let info = reader.info(&args[2]).map_err(|err| err.to_string())?;
            println!("{}", serde_json::to_string_pretty(&info).map_err(|err| err.to_string())?);
            Ok(())
        }
        "local-read" if args.len() == 3 || args.len() == 5 => {
            let registry = Registry::beta_default().map_err(|err| err.to_string())?;
            let reader = LocalReader::from_registry(&registry, &args[1]).map_err(|err| err.to_string())?;
            let (from, to) = if args.len() == 5 {
                let from = args[3].parse::<usize>().map_err(|_| "invalid start line number")?;
                let to = args[4].parse::<usize>().map_err(|_| "invalid end line number")?;
                (Some(from), Some(to))
            } else { (None, None) };
            let file = reader.read(&args[2], from, to).map_err(|err| err.to_string())?;
            println!("{}", serde_json::to_string_pretty(&file).map_err(|err| err.to_string())?);
            Ok(())
        }
        "local-read-many" if args.len() >= 3 && args.len() <= 22 => {
            let registry = Registry::beta_default().map_err(|err| err.to_string())?;
            let reader = LocalReader::from_registry(&registry, &args[1]).map_err(|err| err.to_string())?;
            let result = reader.many(&args[2..]).map_err(|err| err.to_string())?;
            println!("{}", serde_json::to_string_pretty(&result).map_err(|err| err.to_string())?);
            Ok(())
        }
        "local-search-files" | "local-search-text" if args.len() == 3 || args.len() == 4 => {
            let registry = Registry::beta_default().map_err(|err| err.to_string())?;
            let reader = LocalReader::from_registry(&registry, &args[1])
                .map_err(|err| err.to_string())?;
            let limit = if let Some(raw) = args.get(3) {
                raw.parse::<usize>().map_err(|_| "invalid result limit")?
            } else {
                100
            };
            let value = if command == "local-search-files" {
                serde_json::to_value(reader.search_files(&args[2], limit)
                    .map_err(|err| err.to_string())?).map_err(|err| err.to_string())?
            } else {
                serde_json::to_value(reader.search_text(&args[2], limit)
                    .map_err(|err| err.to_string())?).map_err(|err| err.to_string())?
            };
            println!("{}", serde_json::to_string_pretty(&value).map_err(|err| err.to_string())?);
            Ok(())
        }
        "local-edit" if args.len() == 6 || args.len() == 7 && args[6] == "--all" => {
            let registry = Registry::beta_default().map_err(|err| err.to_string())?;
            let reader = LocalReader::from_registry(&registry, &args[1])
                .map_err(|err| err.to_string())?;
            let result = reader.edit_exact(
                &args[2], &args[3], &args[4], &args[5], args.len() == 7
            ).map_err(|err| err.to_string())?;
            println!("{}", serde_json::to_string_pretty(&result).map_err(|err| err.to_string())?);
            Ok(())
        }
        "sync" if args.len() == 1 => codelocal::pair::sync(),
        "logout" if args.len() == 1 => codelocal::pair::logout(),
        "login" | "pair" if args.len() == 1 => codelocal::pair::login(false),
        "login" | "pair" if args.len() == 2 && args[1] == "--force" => codelocal::pair::login(true),
        "" => Err("CodeLocal Rust device runtime is not implemented yet; run --help for available beta commands".into()),
        "signout" | "switch-account" | "switch" |
        "setup" | "doctor" | "agent" | "approvals" | "mcp" | "stop" | "reset" | "uninstall" => {
            Err(format!("{command}: not implemented in Rust beta; the production Go CLI remains authoritative"))
        }
        other if args.len() == 1 && !other.starts_with('-') && Path::new(other).is_dir() => {
            let registry = Registry::beta_default().map_err(|error| error.to_string())?;
            let ws = registry.grant(other).map_err(|error| error.to_string())?;
            println!("✓ Workspace added: {}\n  ID: {}\n  Path: {}", ws.workspace_name, ws.workspace_id, ws.local_path.display());
            println!("Beta grant saved locally. Run `codelocal sync` to sync metadata; remote runtime is not yet implemented.");
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
