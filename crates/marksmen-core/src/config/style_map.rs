//! Global Word style mapping for DOCX output.
//!
//! ## Design contract
//!
//! `StyleMap` is a typed dictionary that maps standard Markdown structural
//! elements to specific Word paragraph style names. It is populated from the
//! YAML frontmatter `style_map:` block and carried on [`Config`].
//!
//! When a field is `None`, the DOCX serializer falls back to the hardcoded
//! default style name for that element (e.g. `"Heading1"` for H1).
//!
//! When a field is `Some(name)`, the serializer uses `name` verbatim as the
//! `w:pStyle w:val` attribute, enabling any named corporate Word style.
//!
//! ## Example frontmatter
//!
//! ```yaml
//! ---
//! title: Architecture Doc
//! style_map:
//!   heading: ["Heading 1", "Heading 2", null, null, null, null]
//!   blockquote: "QuoteBox"
//!   table: "GridTable1Light"
//! ---
//! ```
//!
//! [`Config`]: super::Config

use serde::{Deserialize, Serialize};

/// Typed global style map from YAML frontmatter.
///
/// ## Invariant
///
/// `heading[i]` corresponds to heading level `i + 1` (H1 → index 0, H6 → index 5).
/// Serialized as a YAML sequence of 6 nullable strings so the user only needs to
/// supply the levels they care about — trailing nulls may be omitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleMap {
    /// Word style name overrides for heading levels H1–H6.
    /// Index 0 = H1, index 5 = H6. `None` at any position uses the default.
    #[serde(default = "default_heading_array")]
    pub heading: [Option<String>; 6],

    /// Word style name for blockquote paragraphs. Default: `"Quote"`.
    #[serde(default)]
    pub blockquote: Option<String>,

    /// Word style name for fenced code block paragraphs. Default: `"CodeBlock"`.
    #[serde(default)]
    pub code_block: Option<String>,

    /// Word table style name applied via `w:tblStyle`. Default: none (uses
    /// docx-rs table defaults).
    #[serde(default)]
    pub table: Option<String>,

    /// Word style name for body paragraphs. Default: docx-rs document default.
    #[serde(default)]
    pub paragraph: Option<String>,
}

fn default_heading_array() -> [Option<String>; 6] {
    [None, None, None, None, None, None]
}

impl Default for StyleMap {
    fn default() -> Self {
        Self {
            heading: default_heading_array(),
            blockquote: None,
            code_block: None,
            table: None,
            paragraph: None,
        }
    }
}

impl StyleMap {
    /// Returns the effective heading style name for the given level (1-indexed).
    ///
    /// Falls back to the conventional name `"HeadingN"` when no override is set.
    ///
    /// # Panics
    ///
    /// Never panics — `level` is clamped to `[1, 6]`.
    pub fn heading_style(&self, level: usize) -> &str {
        static DEFAULTS: [&str; 6] = [
            "Heading1", "Heading2", "Heading3", "Heading4", "Heading5", "Heading6",
        ];
        let idx = level.saturating_sub(1).min(5);
        self.heading[idx]
            .as_deref()
            .unwrap_or(DEFAULTS[idx])
    }

    /// Returns the effective blockquote style name. Default: `"Quote"`.
    pub fn blockquote_style(&self) -> &str {
        self.blockquote.as_deref().unwrap_or("Quote")
    }

    /// Returns the effective code block style name. Default: `"CodeBlock"`.
    pub fn code_block_style(&self) -> &str {
        self.code_block.as_deref().unwrap_or("CodeBlock")
    }
}

#[cfg(test)]
mod tests {
    use super::StyleMap;

    #[test]
    fn default_heading_style_falls_back_to_conventional_name() {
        let sm = StyleMap::default();
        assert_eq!(sm.heading_style(1), "Heading1");
        assert_eq!(sm.heading_style(6), "Heading6");
    }

    #[test]
    fn heading_style_override_is_returned() {
        let mut sm = StyleMap::default();
        sm.heading[0] = Some("Heading 1".to_string());
        assert_eq!(sm.heading_style(1), "Heading 1");
        // H2 still falls back.
        assert_eq!(sm.heading_style(2), "Heading2");
    }

    #[test]
    fn blockquote_falls_back_to_quote() {
        let sm = StyleMap::default();
        assert_eq!(sm.blockquote_style(), "Quote");
    }

    #[test]
    fn blockquote_override_is_returned() {
        let sm = StyleMap { blockquote: Some("QuoteBox".to_string()), ..Default::default() };
        assert_eq!(sm.blockquote_style(), "QuoteBox");
    }

    #[test]
    fn serde_roundtrip_preserves_overrides() {
        let yaml = r#"
heading: ["Heading 1", "Heading 2", null, null, null, null]
blockquote: "QuoteBox"
table: "GridTable1Light"
"#;
        let sm: StyleMap = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(sm.heading_style(1), "Heading 1");
        assert_eq!(sm.heading_style(2), "Heading 2");
        assert_eq!(sm.heading_style(3), "Heading3");
        assert_eq!(sm.blockquote.as_deref(), Some("QuoteBox"));
        assert_eq!(sm.table.as_deref(), Some("GridTable1Light"));
    }

    #[test]
    fn config_merge_frontmatter_propagates_style_map() {
        use crate::config::{Config, FrontMatterConfig};
        let mut fm = FrontMatterConfig::default();
        let mut sm = StyleMap::default();
        sm.heading[0] = Some("Corporate H1".to_string());
        fm.style_map = Some(sm);

        let base = Config::default();
        let merged = base.merge_frontmatter(&fm);
        assert_eq!(merged.style_map.heading_style(1), "Corporate H1");
        assert_eq!(merged.style_map.heading_style(2), "Heading2");
    }
}
