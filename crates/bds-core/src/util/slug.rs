use deunicode::deunicode;
use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a URL-safe slug from a title string.
///
/// Transliterates Unicode to ASCII, lowercases, replaces non-alphanumeric
/// chars with hyphens, and collapses/trims hyphens.
pub fn slugify(input: &str) -> String {
    let ascii = deunicode(input);
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
/// tries base, then {slug}-2 .. {slug}-999, then {slug}-{timestamp}.
///
/// `exists` is a predicate that returns true if the candidate slug is already taken.
pub fn ensure_unique<F>(base: &str, exists: F) -> String
where
    F: Fn(&str) -> bool,
{
    if !exists(base) {
        return base.to_string();
    }
    for n in 2..=999 {
        let candidate = format!("{base}-{n}");
        if !exists(&candidate) {
            return candidate;
        }
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{base}-{ts}")
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
        let slug = ensure_unique("hello", |s| s == "hello" || s == "hello-2" || s == "hello-3");
        assert_eq!(slug, "hello-4");
    }

    #[test]
    fn ensure_unique_all_999_taken() {
        let slug = ensure_unique("x", |s| {
            if s == "x" { return true; }
            if let Some(suffix) = s.strip_prefix("x-") {
                if let Ok(n) = suffix.parse::<u32>() {
                    return n <= 999;
                }
            }
            false
        });
        // Should fall back to timestamp-based slug
        assert!(slug.starts_with("x-"));
        let suffix = slug.strip_prefix("x-").unwrap();
        let ts: u64 = suffix.parse().expect("should be a timestamp");
        assert!(ts > 1_000_000_000);
    }
}
