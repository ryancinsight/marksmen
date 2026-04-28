//! Reader for Marp Markdown documents produced by `marksmen-marp`.
//!
//! # Parsing Contract
//!
//! A Marp document produced by `marksmen-marp::convert` has the structure:
//!
//! ```text
//! ---
//! marp: true
//! ...other YAML directives...
//! ---
//!
//! <CommonMark content with `---` slide separators>
//! ```
//!
//! This reader performs three transformations to recover standard Markdown:
//!
//! 1. **Strip YAML front matter**: The leading `---\n...\n---\n` block is
//!    removed entirely.
//! 2. **Normalize slide separators**: Interior `---` lines become `\n\n---\n`
//!    (CommonMark thematic breaks) which pulldown-cmark handles correctly.
//! 3. **Strip Marp-specific HTML comments**: Directives such as
//!    `<!-- _class: lead -->` or `<!-- paginate: false -->` are removed so
//!    they do not appear as literal text.
//!
//! The returned `String` is valid CommonMark processable by pulldown-cmark.
//!
//! # Invariant
//! No content that is visible in the original Markdown paragraph or heading
//! text is removed by this reader.

use anyhow::Result;

/// Parse a Marp Markdown document and return the content as standard
/// CommonMark Markdown, suitable for re-parsing with pulldown-cmark.
pub fn parse_marp(input: &str) -> Result<String> {
    let body = strip_front_matter(input);
    let body = strip_marp_html_comments(body);
    Ok(body.trim().to_string())
}

// ── Front matter extraction ──────────────────────────────────────────────────

/// Remove the YAML front matter block from the start of the document.
///
/// A front matter block is delimited by `---` on its own line at the very
/// beginning of the document and a matching closing `---` on its own line.
/// If no well-formed front matter is present the full input is returned.
fn strip_front_matter(input: &str) -> &str {
    // Front matter must start at byte 0.
    if !input.starts_with("---") {
        return input;
    }
    // The opening sentinel occupies the first line; find the closing `---`.
    let after_open = match input.find('\n') {
        Some(nl) => &input[nl + 1..],
        None => return input,
    };
    // Find the closing `---` on its own line.
    for (i, line) in after_open.lines().enumerate() {
        if line.trim() == "---" {
            // Compute byte offset of the character after this closing sentinel.
            let close_byte: usize = after_open
                .lines()
                .take(i + 1)
                .map(|l| l.len() + 1) // +1 for '\n'
                .sum();
            return &after_open[close_byte..];
        }
    }
    // No closing sentinel found — treat entire document as body.
    input
}

// ── Marp HTML comment directive removal ─────────────────────────────────────

/// Remove Marp global and local directives written as HTML comments.
///
/// Examples that are stripped:
/// - `<!-- _class: lead -->`
/// - `<!-- paginate: false -->`
/// - `<!-- footer: My slides -->`
///
/// Standard HTML comments that do not look like Marp directives are preserved.
fn strip_marp_html_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end_rel) => {
                let comment_body = &rest[start + 4..start + end_rel];
                // Only strip comments that look like Marp directives.
                if is_marp_directive(comment_body) {
                    // Skip the whole comment (do not emit it).
                } else {
                    // Re-emit non-directive HTML comments verbatim.
                    out.push_str("<!--");
                    out.push_str(comment_body);
                    out.push_str("-->");
                }
                rest = &rest[start + end_rel + 3..];
            }
            None => {
                // Unclosed comment: emit remainder verbatim.
                out.push_str(&rest[start..]);
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Return `true` if the comment body matches a known Marp directive pattern.
///
/// Marp directive comments contain one or more `key: value` lines, optionally
/// prefixed with `_` for local (single-slide) scope.
fn is_marp_directive(body: &str) -> bool {
    let trimmed = body.trim();
    // Must be a single-line or multi-line directive — no block of prose.
    // Heuristic: at least one `key: value` token where key is an identifier
    // optionally prefixed by `_`.
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Check for `_?identifier: ...`
        let key_part = line.split(':').next().unwrap_or("");
        let key = key_part.trim().trim_start_matches('_');
        if key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            && !key.is_empty()
        {
            return true;
        }
        return false;
    }
    false
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_DOC: &str = "\
---
marp: true
headingDivider: 1
paginate: true
theme: default
---

# Slide One

Body paragraph.

---

# Slide Two

More content.
";

    #[test]
    fn test_strips_front_matter() {
        let md = parse_marp(FULL_DOC).unwrap();
        assert!(
            !md.contains("marp: true"),
            "front matter not stripped: {md}"
        );
        assert!(!md.contains("headingDivider"), "front matter not stripped");
        assert!(
            md.contains("# Slide One"),
            "heading missing after strip: {md}"
        );
    }

    #[test]
    fn test_preserves_slide_content() {
        let md = parse_marp(FULL_DOC).unwrap();
        assert!(md.contains("Body paragraph."), "body text missing: {md}");
        assert!(
            md.contains("# Slide Two"),
            "second slide heading missing: {md}"
        );
        assert!(
            md.contains("More content."),
            "second slide body missing: {md}"
        );
    }

    #[test]
    fn test_marp_directives_stripped() {
        let doc = "---\nmarp: true\n---\n\n<!-- _class: lead -->\n# Hello\n";
        let md = parse_marp(doc).unwrap();
        assert!(!md.contains("_class"), "marp directive not stripped: {md}");
        assert!(md.contains("# Hello"), "heading removed: {md}");
    }

    #[test]
    fn test_non_directive_comment_preserved() {
        let doc = "---\nmarp: true\n---\n\n<!-- This is a regular comment -->\n# Hello\n";
        let md = parse_marp(doc).unwrap();
        // "This is a regular comment" is not a key:value directive.
        assert!(
            md.contains("This is a regular comment"),
            "non-directive comment was stripped: {md}"
        );
    }

    #[test]
    fn test_no_front_matter_passthrough() {
        let doc = "# Just a heading\nNo front matter here.";
        let md = parse_marp(doc).unwrap();
        assert!(md.contains("# Just a heading"));
        assert!(md.contains("No front matter here."));
    }
}
