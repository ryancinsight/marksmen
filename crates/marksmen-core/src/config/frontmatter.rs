//! YAML front-matter parsing from markdown documents.
//!
//! Extracts a YAML block delimited by `---` at the start of the document
//! and deserializes it into a `FrontMatterConfig`.

use anyhow::{Context, Result};

use super::FrontMatterConfig;

/// Parse front-matter from a markdown string.
///
/// Returns `(body, config)` where `body` is the markdown content after
/// the front-matter block, and `config` is the parsed configuration.
///
/// If no front-matter is present (document doesn't start with `---`),
/// the entire string is returned as the body with a default config.
pub fn parse_frontmatter(markdown: &str) -> Result<(&str, FrontMatterConfig)> {
    let trimmed = markdown.trim_start();

    if !trimmed.starts_with("---") {
        return Ok((markdown, FrontMatterConfig::default()));
    }

    // Find the closing `---` delimiter.
    let after_opening = &trimmed[3..];
    let closing_pos = after_opening
        .find("\n---")
        .or_else(|| after_opening.find("\r\n---"));

    match closing_pos {
        Some(pos) => {
            let yaml_block = &after_opening[..pos].trim();
            let body_start = after_opening[pos..].find('\n').map(|n| pos + n + 1);

            // Find the end of the closing --- line
            let body = match body_start {
                Some(start) => {
                    let after_close = &after_opening[start..];
                    // Skip the --- line itself
                    match after_close.strip_prefix("---") {
                        Some(rest) => rest
                            .strip_prefix('\n')
                            .or_else(|| rest.strip_prefix("\r\n"))
                            .unwrap_or(rest),
                        None => after_close,
                    }
                }
                None => "",
            };

            let config: FrontMatterConfig =
                serde_yaml::from_str(yaml_block).context("Failed to parse YAML front-matter")?;

            Ok((body, config))
        }
        None => {
            // No closing delimiter found — treat entire document as body.
            tracing::warn!("Front-matter opening `---` found but no closing delimiter");
            Ok((markdown, FrontMatterConfig::default()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frontmatter() {
        let (body, config) = parse_frontmatter("# Hello\n\nWorld").unwrap();
        assert_eq!(body, "# Hello\n\nWorld");
        assert!(config.title.is_none());
    }

    #[test]
    fn basic_frontmatter() {
        let md = "---\ntitle: Test Doc\n---\n# Hello";
        let (body, config) = parse_frontmatter(md).unwrap();
        assert_eq!(config.title.as_deref(), Some("Test Doc"));
        assert!(body.contains("# Hello"));
    }

    #[test]
    fn frontmatter_with_math_disabled() {
        let md = "---\nmath:\n  enabled: false\n---\n# No Math";
        let (body, config) = parse_frontmatter(md).unwrap();
        assert_eq!(config.math.as_ref().map(|m| m.enabled), Some(false));
        assert!(body.contains("# No Math"));
    }

    #[test]
    fn unclosed_frontmatter_returns_full_body() {
        let md = "---\ntitle: Broken\n# Heading";
        let (body, _config) = parse_frontmatter(md).unwrap();
        assert_eq!(body, md);
    }
}
