use std::io::Read as _;
use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = match bds_cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let success = matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            let _ = error.print();
            return if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            };
        }
    };

    let json = cli.json;
    let needs_stdin = match &cli.command {
        bds_cli::Command::Post(args) => args.stdin,
        bds_cli::Command::Gallery(args) => args.post.stdin,
        _ => false,
    };
    let mut context = bds_cli::RunContext::system();
    if needs_stdin && let Err(error) = std::io::stdin().read_to_string(&mut context.stdin) {
        eprintln!("Error: could not read stdin: {error}");
        return ExitCode::from(1);
    }

    match bds_cli::run(cli, context) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            if json {
                eprintln!(
                    "{}",
                    serde_json::json!({"ok": false, "error": format!("{error:#}")})
                );
            } else {
                eprintln!("Error: {error:#}");
            }
            ExitCode::from(1)
        }
    }
}
