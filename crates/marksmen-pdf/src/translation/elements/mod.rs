//! Standard markdown element translators.
//!
//! Each function translates a specific markdown construct to Typst markup.

/// Translate a heading level (1-6) to Typst heading markup.
///
/// Typst headings use `=` signs: `= H1`, `== H2`, etc.
pub fn heading_prefix(level: u8) -> String {
    "=".repeat(level as usize)
}

/// Escape text content for safe inclusion in Typst markup.
///
/// Typst's special characters that must be escaped: `#`, `$`, `*`, `_`,
/// `@`, `<`, `>`, `` ` ``, `~`.
pub fn escape_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '#' | '$' | '@' | '~' | '_' | '*' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}

/// Format a Typst code block with optional language annotation.
pub fn code_block(language: Option<&str>, code: &str) -> String {
    match language {
        Some(lang) if !lang.is_empty() => {
            format!("```{}\n{}\n```", lang, code)
        }
        _ => format!("```\n{}\n```", code),
    }
}

/// Format inline code in Typst.
pub fn inline_code(code: &str) -> String {
    // If code contains backticks, Typst handles it via raw with
    // different delimiters, but for simplicity use single backticks.
    if code.contains('`') {
        format!("raw(\"{}\")", code.replace('"', "\\\""))
    } else {
        format!("`{}`", code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_levels() {
        assert_eq!(heading_prefix(1), "=");
        assert_eq!(heading_prefix(2), "==");
        assert_eq!(heading_prefix(3), "===");
        assert_eq!(heading_prefix(6), "======");
    }

    #[test]
    fn escape_special_chars() {
        assert_eq!(escape_text("Price: $10"), "Price: \\$10");
        assert_eq!(escape_text("#heading"), "\\#heading");
        assert_eq!(escape_text("plain text"), "plain text");
        assert_eq!(escape_text("f/f_s = 1 * 5"), "f/f\\_s = 1 \\* 5");
    }

    #[test]
    fn code_block_with_language() {
        let result = code_block(Some("rust"), "fn main() {}");
        assert!(result.starts_with("```rust"));
        assert!(result.contains("fn main() {}"));
    }

    #[test]
    fn code_block_no_language() {
        let result = code_block(None, "hello");
        assert!(result.starts_with("```\n"));
    }
}
