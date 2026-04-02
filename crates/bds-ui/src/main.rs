use bds_ui::BdsApp;

fn main() -> iced::Result {
    iced::application("bDS", BdsApp::update, BdsApp::view)
        .subscription(BdsApp::subscription)
        .run_with(BdsApp::new)
}
