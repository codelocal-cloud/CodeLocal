use std::process::ExitCode;

fn main() -> ExitCode {
    let command = std::env::args().nth(1).unwrap_or_default();
    match command.as_str() {
        "--version" | "-V" | "version" => {
            println!("codelocal {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "--help" | "-h" | "help" | "" => {
            println!("CodeLocal beta Rust CLI");
            println!("Usage: codelocal [--version|--help]");
            println!(
                "Workspace pairing, MCP tools, and local execution are not available in beta bootstrap."
            );
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Command not implemented in Rust beta: {command}");
            ExitCode::FAILURE
        }
    }
}
