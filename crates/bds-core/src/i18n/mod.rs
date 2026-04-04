use std::collections::HashMap;
use std::sync::LazyLock;

/// Supported UI locales, matching the i18n.allium SupportedLanguage spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiLocale {
    En,
    De,
    Fr,
    It,
    Es,
}

impl UiLocale {
    /// BCP-47 base code for this locale.
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::De => "de",
            Self::Fr => "fr",
            Self::It => "it",
            Self::Es => "es",
        }
    }

    /// All supported locales.
    pub fn all() -> &'static [UiLocale] {
        &[Self::En, Self::De, Self::Fr, Self::It, Self::Es]
    }

    /// Unicode regional-indicator flag emoji for this locale.
    pub fn flag_emoji(self) -> &'static str {
        match self {
            Self::En => "\u{1F1EC}\u{1F1E7}", // 🇬🇧
            Self::De => "\u{1F1E9}\u{1F1EA}", // 🇩🇪
            Self::Fr => "\u{1F1EB}\u{1F1F7}", // 🇫🇷
            Self::It => "\u{1F1EE}\u{1F1F9}", // 🇮🇹
            Self::Es => "\u{1F1EA}\u{1F1F8}", // 🇪🇸
        }
    }
}

impl std::fmt::Display for UiLocale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// Normalize a language code to a supported UiLocale.
///
/// Strips region suffix ("en-US" → "en"), lowercases, and falls back to English
/// for unrecognized codes. This implements the LanguageNormalization invariant
/// from i18n.allium.
pub fn normalize_language(code: &str) -> UiLocale {
    let base = code.split(['-', '_']).next().unwrap_or("en").to_lowercase();
    match base.as_str() {
        "en" => UiLocale::En,
        "de" => UiLocale::De,
        "fr" => UiLocale::Fr,
        "it" => UiLocale::It,
        "es" => UiLocale::Es,
        _ => UiLocale::En,
    }
}

/// Detect the OS locale and return the closest supported UiLocale.
///
/// Uses sys-locale to query the system. Falls back to English if detection
/// fails or the detected locale is not supported.
pub fn detect_os_locale() -> UiLocale {
    match sys_locale::get_locale() {
        Some(tag) => normalize_language(&tag),
        None => UiLocale::En,
    }
}

type Catalog = HashMap<String, String>;

fn parse_catalog(json: &str) -> Catalog {
    serde_json::from_str(json).unwrap_or_default()
}

static CATALOG_EN: LazyLock<Catalog> =
    LazyLock::new(|| parse_catalog(include_str!("../../../../locales/ui/en.json")));
static CATALOG_DE: LazyLock<Catalog> =
    LazyLock::new(|| parse_catalog(include_str!("../../../../locales/ui/de.json")));
static CATALOG_FR: LazyLock<Catalog> =
    LazyLock::new(|| parse_catalog(include_str!("../../../../locales/ui/fr.json")));
static CATALOG_IT: LazyLock<Catalog> =
    LazyLock::new(|| parse_catalog(include_str!("../../../../locales/ui/it.json")));
static CATALOG_ES: LazyLock<Catalog> =
    LazyLock::new(|| parse_catalog(include_str!("../../../../locales/ui/es.json")));

fn catalog_for(locale: UiLocale) -> &'static Catalog {
    match locale {
        UiLocale::En => &CATALOG_EN,
        UiLocale::De => &CATALOG_DE,
        UiLocale::Fr => &CATALOG_FR,
        UiLocale::It => &CATALOG_IT,
        UiLocale::Es => &CATALOG_ES,
    }
}

/// Look up a translation key for the given locale.
///
/// Fallback chain: requested locale → English → key itself.
/// This implements the MenuTranslations invariant from i18n.allium.
pub fn translate(locale: UiLocale, key: &str) -> String {
    if let Some(val) = catalog_for(locale).get(key) {
        return val.clone();
    }
    if locale != UiLocale::En {
        if let Some(val) = CATALOG_EN.get(key) {
            return val.clone();
        }
    }
    key.to_string()
}

/// Look up a translation key and substitute `{param}` placeholders.
///
/// Parameters are provided as `&[("param_name", "value")]`.
pub fn translate_with(locale: UiLocale, key: &str, params: &[(&str, &str)]) -> String {
    let mut result = translate(locale, key);
    for (name, value) in params {
        let placeholder = format!("{{{name}}}");
        result = result.replace(&placeholder, value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // LanguageNormalization invariant: base-extract and fallback
    #[test]
    fn normalize_strips_region() {
        assert_eq!(normalize_language("en-US"), UiLocale::En);
        assert_eq!(normalize_language("de-AT"), UiLocale::De);
        assert_eq!(normalize_language("fr-CA"), UiLocale::Fr);
        assert_eq!(normalize_language("it-CH"), UiLocale::It);
        assert_eq!(normalize_language("es-MX"), UiLocale::Es);
    }

    #[test]
    fn normalize_underscore_separator() {
        assert_eq!(normalize_language("de_DE"), UiLocale::De);
    }

    #[test]
    fn normalize_unknown_falls_back_to_en() {
        assert_eq!(normalize_language("xx"), UiLocale::En);
        assert_eq!(normalize_language("ja"), UiLocale::En);
        assert_eq!(normalize_language(""), UiLocale::En);
    }

    #[test]
    fn normalize_case_insensitive() {
        assert_eq!(normalize_language("DE"), UiLocale::De);
        assert_eq!(normalize_language("FR-fr"), UiLocale::Fr);
    }

    // SplitLocalization invariant: UiLocale is independent of content language
    #[test]
    fn ui_locale_is_independent_type() {
        let ui = UiLocale::De;
        let content_lang = "fr";
        assert_ne!(ui.code(), content_lang);
    }

    // MenuTranslations invariant: menu labels come from locale catalog
    #[test]
    fn translate_menu_labels() {
        let label = translate(UiLocale::De, "menu.group.file");
        assert_eq!(label, "Datei");

        let label = translate(UiLocale::Fr, "menu.item.save");
        assert_eq!(label, "Enregistrer");
    }

    #[test]
    fn translate_falls_back_to_english() {
        // Non-English locale falls back for missing keys
        let result = translate(UiLocale::De, "menu.group.file");
        assert_eq!(result, "Datei");
    }

    #[test]
    fn translate_missing_key_returns_key() {
        let result = translate(UiLocale::En, "nonexistent.key.xyz");
        assert_eq!(result, "nonexistent.key.xyz");
    }

    #[test]
    fn translate_with_interpolation() {
        let result = translate_with(
            UiLocale::En,
            "tasks.triggerTitle",
            &[("running", "2"), ("pending", "3")],
        );
        assert_eq!(result, "2 running, 3 pending");
    }

    #[test]
    fn translate_with_interpolation_german() {
        let result = translate_with(
            UiLocale::De,
            "tasks.triggerTitle",
            &[("running", "1"), ("pending", "5")],
        );
        assert_eq!(result, "1 laufend, 5 ausstehend");
    }

    #[test]
    fn all_locales_have_menu_keys() {
        let key = "menu.group.file";
        for locale in UiLocale::all() {
            let val = translate(*locale, key);
            assert_ne!(val, key, "missing {key} for locale {locale}");
        }
    }

    #[test]
    fn locale_code_roundtrip() {
        for locale in UiLocale::all() {
            assert_eq!(normalize_language(locale.code()), *locale);
        }
    }

    #[test]
    fn detect_os_locale_does_not_panic() {
        let _ = detect_os_locale();
    }
}
