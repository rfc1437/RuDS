#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use bds_ui::BdsApp;
use bds_ui::components::inputs;

fn main() -> iced::Result {
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
        .run_with(BdsApp::new)
}
