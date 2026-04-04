//! App launch smoke tests.
//!
//! M0 validation: verifies that the core UI types can be constructed
//! and that the message routing works at the type level.
//!
//! NOTE: muda::Menu on macOS requires the actual main thread (not just
//! single-threaded test mode). Menu construction and BdsApp::new() cannot
//! be tested via `cargo test`. The full smoke test is launching the binary:
//!   cargo run -p bds-ui

use bds_ui::app::Message;

// ── Smoke: Message enum is well-formed ──

#[test]
fn message_variants_constructable() {
    let _noop = Message::Noop;
    let _menu = Message::MenuEvent(muda::MenuId::new("test"));
    // Verify Debug trait works
    assert!(format!("{:?}", Message::Noop).contains("Noop"));
}

#[test]
fn message_clone_works() {
    let msg = Message::MenuEvent(muda::MenuId::new("file-open"));
    let cloned = msg.clone();
    assert!(format!("{cloned:?}").contains("MenuEvent"));
}

// ── Smoke: BdsApp type is accessible from integration tests ──

#[test]
fn bds_app_type_is_public() {
    // This test verifies the public API surface exists.
    // BdsApp, Message, platform::menu are all reachable.
    fn _assert_types() {
        let _: fn() -> (bds_ui::BdsApp, iced::Task<Message>) = bds_ui::BdsApp::new;
    }
}
