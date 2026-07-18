//! App launch smoke tests.
//!
//! Validates that the core UI types can be constructed
//! and that the message routing works at the type level.
//!
//! NOTE: muda::Menu on macOS requires the actual main thread (not just
//! single-threaded test mode). Menu construction and BdsApp::new() cannot
//! be tested via `cargo test`. The full smoke test is launching the binary:
//!   cargo run -p bds-ui

use bds_core::i18n::UiLocale;
use bds_ui::app::Message;
use bds_ui::state::navigation::{PanelTab, SidebarView};
use bds_ui::state::tabs::{Tab, TabType};
use bds_ui::state::toast::ToastLevel;
use std::path::PathBuf;

// ── Smoke: Message enum is well-formed ──

#[test]
fn message_variants_constructable() {
    let _noop = Message::Noop;
    let _menu = Message::MenuEvent(muda::MenuId::new("test"));
    assert!(format!("{:?}", Message::Noop).contains("Noop"));
}

#[test]
fn message_clone_works() {
    let msg = Message::MenuEvent(muda::MenuId::new("file-open"));
    let cloned = msg.clone();
    assert!(format!("{cloned:?}").contains("MenuEvent"));
}

#[test]
fn new_message_variants_constructable() {
    // Navigation
    let _view = Message::SetActiveView(SidebarView::Posts);
    let _toggle_sb = Message::ToggleSidebar;
    let _toggle_p = Message::TogglePanel;

    // Tabs
    let tab = Tab {
        id: "test".to_string(),
        tab_type: TabType::Post,
        title: "Test".to_string(),
        is_transient: false,
        is_dirty: false,
    };
    let _open = Message::OpenTab(tab);
    let _close = Message::CloseTab("test".into());
    let _select = Message::SelectTab("test".into());
    let _pin = Message::PinTab("test".into());

    // Project
    let _switch = Message::SwitchProject("id".into());
    let _create = Message::CreateProject {
        name: "X".into(),
        data_path: None,
    };
    let _delete = Message::DeleteProject("id".into());

    // Dialogs
    let _folder = Message::FolderPicked(Some(PathBuf::from("/tmp")));
    let _media = Message::MediaFilesPicked(None);

    // Tasks
    let _tick = Message::TaskTick;

    // macOS lifecycle
    let _file = Message::FileOpenRequested(PathBuf::from("/test"));
    let _url = Message::UrlOpenRequested("bds://open".into());

    // Panel
    let _panel = Message::SetPanelTab(PanelTab::Output);

    // Settings
    let _offline = Message::SetOfflineMode(true);
    let _locale = Message::SetUiLocale(UiLocale::De);
    let _toggle_locale = Message::ToggleLocaleDropdown;
    let _toggle_project = Message::ToggleProjectDropdown;
    let _init_menu = Message::InitMenuBar;

    // Blog actions
    let _rebuild = Message::RebuildDatabase;
    let _reindex = Message::ReindexText;
    let _regen_cal = Message::RegenerateCalendar;
    let _validate = Message::ValidateTranslations;
    let _generate = Message::GenerateSite;
    let _diff = Message::RunMetadataDiff;
    let _finished = Message::EngineTaskDone {
        task_id: 1,
        label: "test".into(),
        result: Ok("ok".into()),
    };

    // Toast
    let _show = Message::ShowToast(ToastLevel::Info, "hello".into());
    let _dismiss = Message::DismissToast(1);
    let _expire = Message::ExpireToasts;
}

// ── Smoke: BdsApp type is accessible from integration tests ──

#[test]
fn bds_app_type_is_public() {
    fn _assert_types() {
        let _: fn() -> (bds_ui::BdsApp, iced::Task<Message>) = bds_ui::BdsApp::new;
    }
}
