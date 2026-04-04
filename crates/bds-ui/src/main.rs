use bds_ui::BdsApp;

fn main() -> iced::Result {
    iced::application("bDS", BdsApp::update, BdsApp::view)
        .subscription(BdsApp::subscription)
        .window_size((1200.0, 800.0))
        .run_with(BdsApp::new)
}
