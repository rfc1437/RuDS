#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use bds_server::boot::{BootMode, Platform};
use bds_ui::BdsApp;
use bds_ui::components::inputs;

fn main() -> anyhow::Result<()> {
    let resolved = BootMode::resolve(std::env::var("BDS_MODE").ok().as_deref());
    let env = std::env::vars().collect();
    let mode = resolved.effective(current_platform(), &env);
    if mode != BootMode::Desktop {
        if mode != resolved {
            let locale = bds_core::i18n::detect_os_locale();
            eprintln!(
                "{}",
                bds_core::i18n::translate(locale, "remoteTerminal.headlessFallback")
            );
        }
        let config = bds_server::ServerConfig::from_environment(
            bds_core::util::application_database_path(),
            bds_core::util::application_data_dir(),
        )?;
        return bds_server::run_headless(mode, config);
    }

    let icon =
        iced::window::icon::from_file_data(include_bytes!("../assets/app-icons/bds.png"), None)
            .expect("bundled application icon must be valid");

    iced::application("bDS", BdsApp::update, BdsApp::view)
        .subscription(BdsApp::subscription)
        .theme(|_| inputs::app_theme())
        .window(iced::window::Settings {
            size: iced::Size::new(1200.0, 800.0),
            icon: Some(icon),
            ..Default::default()
        })
        .run_with(BdsApp::new)?;
    Ok(())
}

fn current_platform() -> Platform {
    #[cfg(target_os = "macos")]
    return Platform::MacOs;
    #[cfg(windows)]
    return Platform::Windows;
    #[cfg(all(unix, not(target_os = "macos")))]
    return Platform::Unix;
    #[allow(unreachable_code)]
    Platform::Windows
}
