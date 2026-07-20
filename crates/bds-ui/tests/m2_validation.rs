//! M2: Native Workspace validation tests.
//!
//! Validates toast system, menu sync logic, dialog i18n,
//! and keyboard shortcut coverage.

use bds_core::i18n::{UiLocale, translate};

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

// ── Toast i18n keys ──

#[test]
fn toast_keys_exist_in_all_locales() {
    let keys = [
        "editor.saved",
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
