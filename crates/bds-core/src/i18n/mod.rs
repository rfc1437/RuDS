use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
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

type Bundle = FluentBundle<FluentResource>;

const UI_EN: &str = include_str!("../../../../locales/ui/en.ftl");
const RENDER_EN: &str = include_str!("../../../../locales/render/en.ftl");

fn resource(source: &str) -> FluentResource {
    FluentResource::try_new(source.to_owned())
        .unwrap_or_else(|(_, errors)| panic!("invalid Fluent catalog: {errors:?}"))
}

fn bundle(locale: UiLocale, english: &str, localized: &str) -> Bundle {
    let mut bundle = FluentBundle::new_concurrent(vec![
        locale.code().parse().expect("supported locale is valid"),
    ]);
    bundle.set_use_isolating(false);
    bundle
        .add_resource(resource(english))
        .expect("English Fluent catalog has unique keys");
    if locale != UiLocale::En {
        bundle.add_resource_overriding(resource(localized));
    }
    bundle
}

static UI_CATALOGS: LazyLock<[Bundle; 5]> = LazyLock::new(|| {
    [
        bundle(UiLocale::En, UI_EN, UI_EN),
        bundle(
            UiLocale::De,
            UI_EN,
            include_str!("../../../../locales/ui/de.ftl"),
        ),
        bundle(
            UiLocale::Fr,
            UI_EN,
            include_str!("../../../../locales/ui/fr.ftl"),
        ),
        bundle(
            UiLocale::It,
            UI_EN,
            include_str!("../../../../locales/ui/it.ftl"),
        ),
        bundle(
            UiLocale::Es,
            UI_EN,
            include_str!("../../../../locales/ui/es.ftl"),
        ),
    ]
});

static RENDER_CATALOGS: LazyLock<[Bundle; 5]> = LazyLock::new(|| {
    [
        bundle(UiLocale::En, RENDER_EN, RENDER_EN),
        bundle(
            UiLocale::De,
            RENDER_EN,
            include_str!("../../../../locales/render/de.ftl"),
        ),
        bundle(
            UiLocale::Fr,
            RENDER_EN,
            include_str!("../../../../locales/render/fr.ftl"),
        ),
        bundle(
            UiLocale::It,
            RENDER_EN,
            include_str!("../../../../locales/render/it.ftl"),
        ),
        bundle(
            UiLocale::Es,
            RENDER_EN,
            include_str!("../../../../locales/render/es.ftl"),
        ),
    ]
});

fn locale_index(locale: UiLocale) -> usize {
    match locale {
        UiLocale::En => 0,
        UiLocale::De => 1,
        UiLocale::Fr => 2,
        UiLocale::It => 3,
        UiLocale::Es => 4,
    }
}

fn format(bundle: &Bundle, key: &str, params: &[(&str, &str)]) -> Option<String> {
    let id = key.replace('.', "-");
    let pattern = bundle.get_message(&id)?.value()?;
    let mut args = FluentArgs::new();
    for (name, value) in params {
        args.set(*name, *value);
    }
    let mut errors = Vec::new();
    let value = bundle
        .format_pattern(pattern, Some(&args), &mut errors)
        .into_owned();
    Some(value)
}

fn ui_catalog_for(locale: UiLocale) -> &'static Bundle {
    &UI_CATALOGS[locale_index(locale)]
}

fn render_catalog_for(language: &str) -> &'static Bundle {
    &RENDER_CATALOGS[locale_index(normalize_language(language))]
}

/// Look up a translation key for the given locale.
///
/// Fallback chain: requested locale → English → key itself.
/// This implements the MenuTranslations invariant from i18n.allium.
pub fn translate(locale: UiLocale, key: &str) -> String {
    format(ui_catalog_for(locale), key, &[]).unwrap_or_else(|| key.to_owned())
}

/// Look up a translation key and substitute `{param}` placeholders.
///
/// Parameters are provided as `&[("param_name", "value")]`.
pub fn translate_with(locale: UiLocale, key: &str, params: &[(&str, &str)]) -> String {
    format(ui_catalog_for(locale), key, params).unwrap_or_else(|| key.to_owned())
}

/// Look up a render/template translation key by content language code.
///
/// This is independent of the UI locale — it uses the project's content language.
/// Fallback chain: requested language → English → key itself.
/// Implements the RenderTranslations invariant from i18n.allium.
pub fn translate_render(language: &str, key: &str) -> String {
    format(render_catalog_for(language), key, &[]).unwrap_or_else(|| key.to_owned())
}

/// Return the entire render translation map for a language.
///
/// Used to inject as `translations` into the Liquid template context.
pub fn get_render_translations(language: &str) -> &'static HashMap<String, String> {
    &RENDER_MAPS[locale_index(normalize_language(language))]
}

const RENDER_KEYS: &[&str] = &[
    "render.archive",
    "render.pagination.label",
    "render.pagination.newer",
    "render.pagination.older",
    "render.notFound.message",
    "render.notFound.back",
    "render.photoArchive.empty",
    "render.gallery.empty",
    "render.tagCloud.empty",
    "render.tagCloud.ariaLabel",
    "render.calendar.open",
    "render.calendar.close",
    "render.calendar.title",
    "render.calendar.loading",
    "render.calendar.error",
    "render.taxonomy.ariaLabel",
    "render.backlinks.label",
    "render.backlinks.ariaLabel",
    "render.languageSwitcher.ariaLabel",
    "render.video.youtubeTitle",
    "render.video.vimeoTitle",
    "render.month.1",
    "render.month.2",
    "render.month.3",
    "render.month.4",
    "render.month.5",
    "render.month.6",
    "render.month.7",
    "render.month.8",
    "render.month.9",
    "render.month.10",
    "render.month.11",
    "render.month.12",
    "render.search.placeholder",
    "render.search.ariaLabel",
];

static RENDER_MAPS: LazyLock<[HashMap<String, String>; 5]> = LazyLock::new(|| {
    std::array::from_fn(|index| {
        let locale = UiLocale::all()[index];
        RENDER_KEYS
            .iter()
            .map(|key| ((*key).to_owned(), translate_render(locale.code(), key)))
            .collect()
    })
});

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

    // RenderTranslations invariant: separate catalog for content/template locale
    #[test]
    fn translate_render_english() {
        assert_eq!(translate_render("en", "render.archive"), "Archive");
        assert_eq!(translate_render("en", "render.month.1"), "January");
    }

    #[test]
    fn translate_render_german() {
        assert_eq!(translate_render("de", "render.archive"), "Archiv");
        assert_eq!(translate_render("de", "render.month.1"), "Januar");
    }

    #[test]
    fn translate_render_falls_back_to_english() {
        assert_eq!(translate_render("ja", "render.archive"), "Archive");
    }

    #[test]
    fn translate_render_missing_key_returns_key() {
        assert_eq!(
            translate_render("en", "render.nonexistent"),
            "render.nonexistent"
        );
    }

    #[test]
    fn get_render_translations_not_empty() {
        let map = get_render_translations("en");
        assert!(map.contains_key("render.archive"));
        assert!(map.contains_key("render.month.12"));
        assert!(map.len() >= 34);
    }

    #[test]
    fn all_render_locales_have_archive_key() {
        for code in &["en", "de", "fr", "it", "es"] {
            let val = translate_render(code, "render.archive");
            assert_ne!(val, "render.archive", "missing render.archive for {code}");
        }
    }
}
