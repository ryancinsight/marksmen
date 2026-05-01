//! Markdown parser using `pulldown-cmark` with math and GFM extensions.
//!
//! ## Theorem: Completeness of Event Coverage
//!
//! The parser enables the following pulldown-cmark options:
//! - `ENABLE_MATH`: Emits `InlineMath` and `DisplayMath` events for `$...$` and `$$...$$`
//! - `ENABLE_TABLES`: GFM table support
//! - `ENABLE_STRIKETHROUGH`: `~~text~~` strikethrough
//! - `ENABLE_TASKLISTS`: `- [x]` task lists
//! - `ENABLE_HEADING_ATTRIBUTES`: `{#id .class}` heading attributes
//!
//! All CommonMark constructs are supported by default.

use pulldown_cmark::{Event, Options, Parser};

/// Parse a Markdown string into a `pulldown-cmark` event iterator.
///
/// Enables math, tables, strikethrough, tasklists, and heading attributes
/// in addition to the default CommonMark support.
pub fn parse(markdown: &str) -> Vec<Event<'_>> {
    let options = Options::ENABLE_MATH
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_FOOTNOTES;

    Parser::new_ext(markdown, options).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::{Event, Tag};

    #[test]
    fn parses_inline_math() {
        let events = parse("Inline $x^2$ math");
        let has_inline_math = events.iter().any(|e| matches!(e, Event::InlineMath(_)));
        assert!(has_inline_math, "Expected InlineMath event for $x^2$");
    }

    #[test]
    fn parses_display_math() {
        let events = parse("$$\\int_0^1 x\\,dx$$");
        let has_display_math = events.iter().any(|e| matches!(e, Event::DisplayMath(_)));
        assert!(has_display_math, "Expected DisplayMath event for $$...$$");
    }

    #[test]
    fn parses_heading() {
        let events = parse("# Hello");
        let has_heading = events.iter().any(|e| {
            matches!(
                e,
                Event::Start(Tag::Heading {
                    level: pulldown_cmark::HeadingLevel::H1,
                    ..
                })
            )
        });
        assert!(has_heading, "Expected H1 heading event");
    }

    #[test]
    fn parses_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let events = parse(md);
        let has_table = events
            .iter()
            .any(|e| matches!(e, Event::Start(Tag::Table(_))));
        assert!(has_table, "Expected Table event");
    }
}
