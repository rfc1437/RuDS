use super::timestamp::year_month_from_unix_ms;

/// Post file path: `posts/YYYY/MM/{slug}.md` from `created_at` unix ms.
pub fn post_file_path(created_at_ms: i64, slug: &str) -> String {
    let (y, m) = year_month_from_unix_ms(created_at_ms);
    format!("posts/{y}/{m}/{slug}.md")
}

/// Translation file path: `posts/YYYY/MM/{slug}.{lang}.md` from canonical
/// post's `created_at`.
pub fn translation_file_path(
    canonical_created_at_ms: i64,
    canonical_slug: &str,
    language: &str,
) -> String {
    let (y, m) = year_month_from_unix_ms(canonical_created_at_ms);
    format!("posts/{y}/{m}/{canonical_slug}.{language}.md")
}

/// Media directory path: `media/YYYY/MM/` from `created_at`.
pub fn media_dir_path(created_at_ms: i64) -> String {
    let (y, m) = year_month_from_unix_ms(created_at_ms);
    format!("media/{y}/{m}/")
}

/// Media sidecar path: `{binary_file_path}.meta`.
pub fn media_sidecar_path(file_path: &str) -> String {
    format!("{file_path}.meta")
}

/// Media translation sidecar path: `{binary_file_path}.{lang}.meta`.
pub fn media_translation_sidecar_path(file_path: &str, language: &str) -> String {
    format!("{file_path}.{language}.meta")
}

/// Thumbnail path: `thumbnails/{id[0..2]}/{id}-{size}.{ext}`.
pub fn thumbnail_path(id: &str, size: &str, ext: &str) -> String {
    let prefix = &id[..2.min(id.len())];
    format!("thumbnails/{prefix}/{id}-{size}.{ext}")
}

/// Template file path: `templates/{slug}.liquid`.
pub fn template_file_path(slug: &str) -> String {
    format!("templates/{slug}.liquid")
}

/// Script file path: `scripts/{slug}.lua`.
pub fn script_file_path(slug: &str) -> String {
    format!("scripts/{slug}.lua")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_path_from_fixture() {
        // esmeralda created_at: 2005-11-13T12:00:00.000Z = 1131883200000
        assert_eq!(
            post_file_path(1131883200000, "esmeralda"),
            "posts/2005/11/esmeralda.md"
        );
    }

    #[test]
    fn post_path_march_2026() {
        // ghostty created_at: 2026-03-13
        let ms = 1773583200000_i64; // approx 2026-03-13
        let path = post_file_path(ms, "ghostty");
        assert!(path.starts_with("posts/2026/03/"));
        assert!(path.ends_with("ghostty.md"));
    }

    #[test]
    fn translation_path() {
        assert_eq!(
            translation_file_path(1131883200000, "esmeralda", "en"),
            "posts/2005/11/esmeralda.en.md"
        );
    }

    #[test]
    fn media_paths() {
        assert_eq!(
            media_sidecar_path("media/2005/11/eb0cf9d7.jpg"),
            "media/2005/11/eb0cf9d7.jpg.meta"
        );
        assert_eq!(
            media_translation_sidecar_path("media/2005/11/eb0cf9d7.jpg", "fr"),
            "media/2005/11/eb0cf9d7.jpg.fr.meta"
        );
    }

    #[test]
    fn thumbnail_paths() {
        assert_eq!(
            thumbnail_path("eb0cf9d7-6fbd-4b74-9be3-759d6e16f240", "small", "webp"),
            "thumbnails/eb/eb0cf9d7-6fbd-4b74-9be3-759d6e16f240-small.webp"
        );
        assert_eq!(
            thumbnail_path("eb0cf9d7-6fbd-4b74-9be3-759d6e16f240", "ai", "jpg"),
            "thumbnails/eb/eb0cf9d7-6fbd-4b74-9be3-759d6e16f240-ai.jpg"
        );
    }

    #[test]
    fn template_and_script_paths() {
        assert_eq!(
            template_file_path("testvorlage"),
            "templates/testvorlage.liquid"
        );
        assert_eq!(script_file_path("bgg_link"), "scripts/bgg_link.lua");
    }
}
