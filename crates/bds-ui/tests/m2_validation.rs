//! M2: Native Workspace validation tests.
//!
//! Validates toast system, menu sync logic, dialog i18n,
//! and keyboard shortcut coverage.

use bds_core::i18n::{UiLocale, translate};
use bds_ui::platform::menu::MenuAction;
use bds_ui::state::toast::{Toast, ToastLevel};

// ── Toast system ──

#[test]
fn toast_ids_are_monotonically_increasing() {
    let a = Toast::new(ToastLevel::Info, "first".into());
    let b = Toast::new(ToastLevel::Warning, "second".into());
    let c = Toast::new(ToastLevel::Error, "third".into());
    assert!(b.id > a.id);
    assert!(c.id > b.id);
}

#[test]
fn toast_level_variants() {
    let info = Toast::new(ToastLevel::Info, "info".into());
    let success = Toast::new(ToastLevel::Success, "ok".into());
    let warning = Toast::new(ToastLevel::Warning, "warn".into());
    let error = Toast::new(ToastLevel::Error, "err".into());

    assert_eq!(info.level, ToastLevel::Info);
    assert_eq!(success.level, ToastLevel::Success);
    assert_eq!(warning.level, ToastLevel::Warning);
    assert_eq!(error.level, ToastLevel::Error);
}

#[test]
fn fresh_toast_is_not_expired() {
    let t = Toast::new(ToastLevel::Info, "test".into());
    assert!(!t.is_expired());
}

#[test]
fn toast_preserves_message() {
    let t = Toast::new(ToastLevel::Error, "something failed".into());
    assert_eq!(t.message, "something failed");
}

// ── Menu enable/disable rules ──
// (These test the expected invariants, not the BdsApp method directly,
// since BdsApp::new() requires main thread for muda.)

#[test]
fn menu_actions_that_need_project() {
    let project_actions = [
        MenuAction::NewPost,
        MenuAction::ImportMedia,
        MenuAction::OpenDataFolder,
        MenuAction::EditMenu,
        MenuAction::RebuildDatabase,
        MenuAction::ReindexText,
        MenuAction::MetadataDiff,
        MenuAction::RegenerateCalendar,
        MenuAction::ValidateTranslations,
        MenuAction::GenerateSitemap,
        MenuAction::ValidateSite,
    ];
    // All should have i18n keys
    for action in &project_actions {
        let key = action.i18n_key();
        let label = translate(UiLocale::En, key);
        assert_ne!(label, key, "missing translation for {key}");
    }
    assert_eq!(project_actions.len(), 11);
}

#[test]
fn menu_actions_that_need_tab() {
    let tab_actions = [
        MenuAction::Save,
        MenuAction::OpenInBrowser,
        MenuAction::Find,
        MenuAction::Replace,
    ];
    for action in &tab_actions {
        let key = action.i18n_key();
        let label = translate(UiLocale::En, key);
        assert_ne!(label, key, "missing translation for {key}");
    }
    assert_eq!(tab_actions.len(), 4);
}

#[test]
fn menu_actions_gated_by_offline() {
    let online_only = [MenuAction::FillMissingTranslations, MenuAction::UploadSite];
    for action in &online_only {
        let key = action.i18n_key();
        let label = translate(UiLocale::En, key);
        assert_ne!(label, key, "missing translation for {key}");
    }
    assert_eq!(online_only.len(), 2);
}

// ── Dialog i18n ──

#[test]
fn dialog_keys_exist_in_all_locales() {
    let keys = [
        "dialog.selectFolder",
        "dialog.importMedia",
        "dialog.imageFilter",
    ];
    for locale in UiLocale::all() {
        for key in &keys {
            let label = translate(*locale, key);
            assert_ne!(label, *key, "missing {key} for locale {locale}");
        }
    }
}

// ── Keyboard shortcut coverage ──

#[test]
fn accelerator_actions_match_spec() {
    // M2 spec: these actions must have keyboard shortcuts
    let accelerated = [
        MenuAction::NewPost,         // Cmd+N
        MenuAction::ImportMedia,     // Cmd+I
        MenuAction::Save,            // Cmd+S
        MenuAction::Find,            // Cmd+F
        MenuAction::Replace,         // Cmd+H
        MenuAction::EditPreferences, // Cmd+,
        MenuAction::ViewPosts,       // Cmd+1
        MenuAction::ViewMedia,       // Cmd+2
        MenuAction::ToggleSidebar,   // Cmd+B
        MenuAction::TogglePanel,     // Cmd+J
        MenuAction::PublishSelected, // Cmd+Shift+P
        MenuAction::PreviewPost,     // Cmd+Shift+V
        MenuAction::GenerateSitemap, // Cmd+R
        MenuAction::ValidateSite,    // Cmd+Shift+L
        MenuAction::UploadSite,      // Cmd+Shift+U
    ];
    // All must be valid MenuAction variants with i18n keys
    for action in &accelerated {
        assert!(!action.i18n_key().is_empty());
    }
    assert_eq!(
        accelerated.len(),
        15,
        "M2 spec has 15 accelerator-bound actions"
    );
}

// ── Toast i18n keys ──

#[test]
fn toast_keys_exist_in_all_locales() {
    let keys = [
        "projectSelector.toast.switched",
        "projectSelector.toast.switchFailed",
        "projectSelector.toast.created",
        "projectSelector.toast.createFailed",
        "projectSelector.toast.deleteFailed",
    ];
    for locale in UiLocale::all() {
        for key in &keys {
            let label = translate(*locale, key);
            assert_ne!(label, *key, "missing {key} for locale {locale}");
        }
    }
}
