//! Validation functions for template (Liquid) and script (Lua/Python) content.
//! Per template.allium and script.allium, these are pre-publish gates.
//!
//! Current implementation: basic structural checks.
//! When a full Liquid parser crate is added, upgrade `validate_liquid` to
//! attempt a real parse. Similarly for `validate_script` with a Lua parser.

/// Result of a validation check.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl ValidationResult {
    pub fn ok() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
        }
    }

    pub fn fail(errors: Vec<String>) -> Self {
        Self {
            valid: false,
            errors,
        }
    }
}

/// Validate Liquid template content.
/// Per template.allium: "LiquidJS parser must accept the template".
/// Currently performs structural tag-matching. Upgrade to full liquid crate parse later.
pub fn validate_liquid(content: &str) -> ValidationResult {
    let mut errors = Vec::new();

    // Check for unmatched Liquid block tags
    let block_tags = [
        "if", "unless", "for", "case", "capture", "comment", "raw", "paginate", "tablerow",
        "block", "schema",
    ];

    for tag in &block_tags {
        let open_pattern = format!("{{% {tag}");
        let close_pattern = format!("{{% end{tag}");
        let opens = content.matches(&open_pattern).count();
        let closes = content.matches(&close_pattern).count();
        if opens != closes {
            errors.push(format!(
                "Unmatched {{% {tag} %}}: {opens} opens, {closes} closes"
            ));
        }
    }

    // Check for unclosed {{ }}
    let double_opens = content.matches("{{").count();
    let double_closes = content.matches("}}").count();
    if double_opens != double_closes {
        errors.push(format!(
            "Unmatched {{}}: {double_opens} opens, {double_closes} closes"
        ));
    }

    // Check for unclosed {% %}
    let tag_opens = content.matches("{%").count();
    let tag_closes = content.matches("%}").count();
    if tag_opens != tag_closes {
        errors.push(format!(
            "Unmatched {{% %}}: {tag_opens} opens, {tag_closes} closes"
        ));
    }

    if errors.is_empty() {
        ValidationResult::ok()
    } else {
        ValidationResult::fail(errors)
    }
}

/// Validate script content (Lua syntax).
/// Per script.allium: "AST parsing must succeed".
/// Currently performs basic bracket-matching. Upgrade to full Lua parser later.
pub fn validate_script(content: &str) -> ValidationResult {
    let mut errors = Vec::new();

    // Basic bracket matching
    let mut paren_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;

    for (line_num, line) in content.lines().enumerate() {
        // Skip comment lines
        let trimmed = line.trim();
        if trimmed.starts_with("--") {
            continue;
        }
        for ch in line.chars() {
            match ch {
                '(' => paren_depth += 1,
                ')' => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                '[' => bracket_depth += 1,
                ']' => bracket_depth -= 1,
                _ => {}
            }
            if paren_depth < 0 {
                errors.push(format!("Unexpected ')' at line {}", line_num + 1));
                paren_depth = 0;
            }
            if brace_depth < 0 {
                errors.push(format!("Unexpected '}}' at line {}", line_num + 1));
                brace_depth = 0;
            }
            if bracket_depth < 0 {
                errors.push(format!("Unexpected ']' at line {}", line_num + 1));
                bracket_depth = 0;
            }
        }
    }

    if paren_depth != 0 {
        errors.push(format!("Unclosed parentheses: depth {paren_depth}"));
    }
    if brace_depth != 0 {
        errors.push(format!("Unclosed braces: depth {brace_depth}"));
    }
    if bracket_depth != 0 {
        errors.push(format!("Unclosed brackets: depth {bracket_depth}"));
    }

    // Check for unmatched Lua block keywords.
    // In Lua, `for ... do ... end` and `while ... do ... end` each form a
    // single block, so `do` on such lines must NOT be counted as a separate
    // opener.
    let mut block_opens: usize = 0;
    let mut block_closes: usize = 0;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("--") {
            continue;
        }
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        let has_for_or_while = words.iter().any(|w| matches!(*w, "for" | "while"));
        for w in &words {
            match *w {
                "function" | "if" | "for" | "while" | "repeat" => block_opens += 1,
                "do" if !has_for_or_while => block_opens += 1,
                "end" | "until" => block_closes += 1,
                _ => {}
            }
        }
    }

    if block_opens > block_closes {
        errors.push(format!(
            "Possible unclosed blocks: {block_opens} openers, {block_closes} closers"
        ));
    }

    if errors.is_empty() {
        ValidationResult::ok()
    } else {
        ValidationResult::fail(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_liquid_template() {
        let content = r#"
<html>
{% if user %}
  <p>{{ user.name }}</p>
{% endif %}
</html>"#;
        let result = validate_liquid(content);
        assert!(result.valid, "Errors: {:?}", result.errors);
    }

    #[test]
    fn unmatched_liquid_if() {
        let content = "{% if true %}<p>Hello</p>";
        let result = validate_liquid(content);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("if")));
    }

    #[test]
    fn unmatched_liquid_output() {
        let content = "{{ user.name";
        let result = validate_liquid(content);
        assert!(!result.valid);
    }

    #[test]
    fn valid_lua_script() {
        let content = r#"
function render(ctx)
    local result = {}
    for i = 1, 10 do
        result[i] = i * 2
    end
    return result
end
"#;
        let result = validate_script(content);
        assert!(result.valid, "Errors: {:?}", result.errors);
    }

    #[test]
    fn unmatched_lua_parens() {
        let content = "function test(\n  local x = 1\nend";
        let result = validate_script(content);
        assert!(!result.valid);
    }
}
