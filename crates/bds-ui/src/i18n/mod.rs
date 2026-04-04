use bds_core::i18n::{translate, translate_with, UiLocale};

/// Shorthand for translate in view code.
pub fn t(locale: UiLocale, key: &str) -> String {
    translate(locale, key)
}

/// Shorthand for translate_with in view code.
pub fn tw(locale: UiLocale, key: &str, params: &[(&str, &str)]) -> String {
    translate_with(locale, key, params)
}
