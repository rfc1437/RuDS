use std::net::IpAddr;
use std::path::PathBuf;

use bds_server::boot::BootMode;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "bds-server",
    about = "Headless RuDS engine host over authenticated SSH"
)]
struct Args {
    /// SSH listen address. Defaults to loopback; external access must be explicit.
    #[arg(long)]
    bind: Option<IpAddr>,
    /// SSH listen port.
    #[arg(long)]
    port: Option<u16>,
    /// Application database path.
    #[arg(long)]
    database: Option<PathBuf>,
    /// Private application data directory containing SSH key material.
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let data_root = args
        .data_dir
        .unwrap_or_else(bds_core::util::application_data_dir);
    let database_path = args.database.unwrap_or_else(|| data_root.join("bds.db"));
    let mut config = bds_server::ServerConfig::from_environment(database_path, data_root)?;
    if let Some(bind) = args.bind {
        config.bind = bind;
    }
    if let Some(port) = args.port {
        config.port = port;
    }
    bds_server::run_headless(BootMode::Server, config)
}
