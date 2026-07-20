use unicode_normalization::UnicodeNormalization;

/// Apply the one German replacement that canonical decomposition cannot provide.
fn german_transliterate(input: &str) -> String {
    input.replace('ß', "ss")
}

/// Generate a URL-safe slug from a title string.
///
/// Matches bDS2 exactly: replace ß with `ss`, canonically decompose Unicode,
/// discard non-ASCII code points, lowercase, replace non-alphanumeric runs
/// with hyphens, and trim leading/trailing hyphens.
pub fn slugify(input: &str) -> String {
    let preprocessed = german_transliterate(input);
    let ascii = preprocessed
        .nfd()
        .filter(char::is_ascii)
        .collect::<String>();
    let lowered = ascii.to_lowercase();
    let mut slug = String::with_capacity(lowered.len());
    let mut prev_hyphen = true; // avoid leading hyphen
    for c in lowered.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
            prev_hyphen = false;
        } else if !prev_hyphen {
            slug.push('-');
            prev_hyphen = true;
        }
    }
    // Trim trailing hyphen
    if slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// Ensure a slug is unique within a project, using the spec's algorithm:
/// tries base, then unbounded numeric suffixes `{slug}-2`, `{slug}-3`, and so on.
///
/// `exists` is a predicate that returns true if the candidate slug is already taken.
pub fn ensure_unique<F>(base: &str, exists: F) -> String
where
    F: Fn(&str) -> bool,
{
    if !exists(base) {
        return base.to_string();
    }
    for n in 2_u64.. {
        let candidate = format!("{base}-{n}");
        if !exists(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_slug() {
        assert_eq!(slugify("Hello World"), "hello-world");
    }

    #[test]
    fn unicode_slug() {
        assert_eq!(slugify("Über die Brücke"), "uber-die-brucke");
    }

    #[test]
    fn special_chars() {
        assert_eq!(slugify("What's up? (2024)"), "what-s-up-2024");
    }

    #[test]
    fn already_clean() {
        assert_eq!(slugify("already-clean"), "already-clean");
    }

    #[test]
    fn empty_input() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn consecutive_special_chars() {
        assert_eq!(slugify("a --- b"), "a-b");
    }

    #[test]
    fn ensure_unique_base_available() {
        let slug = ensure_unique("hello", |_| false);
        assert_eq!(slug, "hello");
    }

    #[test]
    fn ensure_unique_base_taken() {
        let slug = ensure_unique("hello", |s| s == "hello");
        assert_eq!(slug, "hello-2");
    }

    #[test]
    fn ensure_unique_sequential_taken() {
        let slug = ensure_unique("hello", |s| {
            s == "hello" || s == "hello-2" || s == "hello-3"
        });
        assert_eq!(slug, "hello-4");
    }

    // German corpus copied from the bDS2 golden-master tests.
    #[test]
    fn bds2_german_transliteration_corpus() {
        assert_eq!(slugify("Straße"), "strasse");
        assert_eq!(slugify("Öl"), "ol");
        assert_eq!(slugify("Äpfel"), "apfel");
        assert_eq!(slugify("Über"), "uber");
        assert_eq!(slugify("ÄÖÜäöüß"), "aouaouss");
    }

    #[test]
    fn bds2_nfd_discards_non_ascii_instead_of_transliterating_it() {
        assert_eq!(slugify("Crème 東京 œ"), "creme");
    }

    #[test]
    fn german_umlaut_ae() {
        assert_eq!(slugify("Ärger"), "arger");
    }

    #[test]
    fn german_umlaut_oe() {
        assert_eq!(slugify("Öffnung"), "offnung");
    }

    #[test]
    fn german_umlaut_ue() {
        assert_eq!(slugify("Über"), "uber");
    }

    #[test]
    fn german_eszett() {
        assert_eq!(slugify("Straße"), "strasse");
    }

    #[test]
    fn german_mixed_umlauts() {
        assert_eq!(slugify("Größe über Maße"), "grosse-uber-masse");
    }

    #[test]
    fn german_uppercase_umlauts() {
        assert_eq!(slugify("ÄÖÜ Test"), "aou-test");
    }

    // spec: CreatePost uses Slug.generate(title ?? "untitled")
    // When title is empty/whitespace, slugify should produce "untitled" equivalent
    #[test]
    fn whitespace_only_input() {
        assert_eq!(slugify("   "), "");
    }

    #[test]
    fn leading_trailing_special() {
        assert_eq!(slugify("---hello---"), "hello");
    }

    #[test]
    fn numeric_only() {
        assert_eq!(slugify("2024"), "2024");
    }

    #[test]
    fn ensure_unique_continues_after_999() {
        let slug = ensure_unique("x", |s| {
            if s == "x" {
                return true;
            }
            if let Some(suffix) = s.strip_prefix("x-")
                && let Ok(n) = suffix.parse::<u32>()
            {
                return n <= 999;
            }
            false
        });
        assert_eq!(slug, "x-1000");
    }
}
