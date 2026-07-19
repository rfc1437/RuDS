pub mod auth;
pub mod boot;
pub mod client;
pub mod host;
pub mod protocol;
pub mod transport;

pub use client::{DesktopClient, RemoteTarget};
pub use transport::{ServerConfig, ServerRuntime};

use anyhow::{Result, bail};
use boot::BootMode;

/// Run the headless host until Ctrl+C, optionally attaching the launching
/// terminal in `tui` mode. This path never links or initializes Iced.
pub fn run_headless(mode: BootMode, config: ServerConfig) -> Result<()> {
    if mode == BootMode::Desktop {
        bail!("desktop mode must be started by bds-ui");
    }
    let database_path = config.database_path.clone();
    let runtime = ServerRuntime::start(config)?;
    let locale = bds_core::db::Database::open(&database_path)
        .ok()
        .and_then(|database| {
            bds_core::engine::settings::ui_language(database.conn())
                .ok()
                .flatten()
        })
        .map(|language| bds_core::i18n::normalize_language(&language))
        .unwrap_or_else(bds_core::i18n::detect_os_locale);
    eprintln!(
        "{}",
        bds_core::i18n::translate_with(
            locale,
            "remoteTerminal.serverListening",
            &[("address", &runtime.address().to_string())],
        )
    );
    eprintln!(
        "{}",
        bds_core::i18n::translate_with(
            locale,
            "remoteTerminal.authorizedKeys",
            &[(
                "path",
                &runtime
                    .key_material()
                    .authorized_keys_path
                    .display()
                    .to_string(),
            )],
        )
    );
    if mode == BootMode::Tui {
        host::run_local_terminal(runtime.application_host())?;
    } else {
        let tokio = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        tokio.block_on(tokio::signal::ctrl_c())?;
    }
    runtime.stop()
}
