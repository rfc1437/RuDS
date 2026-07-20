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

    if let bds_cli::Command::Server(args) = &cli.command {
        let data_root = args
            .data_dir
            .clone()
            .unwrap_or_else(bds_core::util::application_data_dir);
        let database_path = args
            .database
            .clone()
            .unwrap_or_else(|| data_root.join("bds.db"));
        let config = match bds_server::ServerConfig::from_environment(database_path, data_root) {
            Ok(mut config) => {
                if let Some(bind) = args.bind {
                    config.bind = bind;
                }
                if let Some(port) = args.port {
                    config.port = port;
                }
                config
            }
            Err(error) => {
                eprintln!("Error: {error:#}");
                return ExitCode::from(1);
            }
        };
        return run_headless(bds_server::boot::BootMode::Server, config);
    }

    if matches!(cli.command, bds_cli::Command::Tui) {
        let data_root = bds_core::util::application_data_dir();
        let config = match bds_server::ServerConfig::from_environment(
            bds_core::util::application_database_path(),
            data_root,
        ) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("Error: {error:#}");
                return ExitCode::from(1);
            }
        };
        return run_headless(bds_server::boot::BootMode::Tui, config);
    }

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

fn run_headless(mode: bds_server::boot::BootMode, config: bds_server::ServerConfig) -> ExitCode {
    match bds_server::run_headless(mode, config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            ExitCode::from(1)
        }
    }
}
