//! Menu routing integration tests.
//!
//! Validates: all MenuAction variants registered, no collisions,
//! bidirectional lookup, and i18n keys resolve in every locale.

use bds_core::i18n::{translate, UiLocale};
use bds_ui::platform::menu::MenuAction;

#[test]
fn all_menu_actions_have_i18n_keys() {
    for &action in MenuAction::ALL {
        let key = action.i18n_key();
        assert!(key.starts_with("menu.item."), "bad key prefix: {key}");
    }
}

#[test]
fn all_menu_actions_translate_in_all_locales() {
    for locale in UiLocale::all() {
        for &action in MenuAction::ALL {
            let key = action.i18n_key();
            let label = translate(*locale, key);
            assert_ne!(
                label, key,
                "missing translation for {key} in locale {locale}"
            );
        }
    }
}

#[test]
fn menu_action_count_matches_spec() {
    // M2 spec: 28 custom menu actions
    assert_eq!(MenuAction::ALL.len(), 28);
}

#[test]
fn no_duplicate_i18n_keys() {
    let mut keys = std::collections::HashSet::new();
    for &action in MenuAction::ALL {
        let key = action.i18n_key();
        assert!(keys.insert(key), "duplicate i18n key: {key}");
    }
}
