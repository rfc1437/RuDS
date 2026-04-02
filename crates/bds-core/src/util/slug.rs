use deunicode::deunicode;

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
}
