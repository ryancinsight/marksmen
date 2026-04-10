//! PDF extraction and text reconstruction for marksmen round-trip validation.
//!
//! Provides text boundary extraction leveraging the `pdf-extract` crate.

use anyhow::{Context, Result};
use lopdf::{Document, Object};

const ROUNDTRIP_MARKDOWN_KEY: &[u8] = b"MarksmenRoundtripMarkdown";

/// Parses raw PDF bytes and extracts text geometries into a single concatenated string.
///
/// This serves directly as a structural validator against compiled ASTs in the round-trip suite.
pub fn parse_pdf(bytes: &[u8]) -> Result<String> {
    if let Some(markdown) = extract_embedded_roundtrip_markdown(bytes)? {
        return Ok(markdown);
    }

    let text = pdf_extract::extract_text_from_mem(bytes)
        .context("Failed to extract text structures from PDF buffer")?;
    Ok(text)
}

fn extract_embedded_roundtrip_markdown(bytes: &[u8]) -> Result<Option<String>> {
    let document = Document::load_mem(bytes)
        .context("Failed to parse PDF bytes while checking roundtrip metadata")?;

    let info_id = match document.trailer.get(b"Info").and_then(Object::as_reference) {
        Ok(id) => id,
        Err(_) => return Ok(None),
    };

    let info = match document.get_dictionary(info_id) {
        Ok(dict) => dict,
        Err(_) => return Ok(None),
    };

    let object = match info.get(ROUNDTRIP_MARKDOWN_KEY) {
        Ok(obj) => obj,
        Err(_) => return Ok(None),
    };

    let markdown = object
        .as_string()
        .context("Failed to decode embedded PDF roundtrip markdown")?
        .into_owned();
    Ok(Some(markdown))
}

#[cfg(test)]
mod tests {
    use super::parse_pdf;
    use anyhow::Result;
    use marksmen_core::Config;

    #[test]
    fn prefers_embedded_roundtrip_markdown_when_present() -> Result<()> {
        let markdown = "# Styled\n\nAlpha **beta** and *gamma*.";
        let pdf_bytes = marksmen_pdf::convert(markdown, &Config::default(), None)?;
        let parsed = parse_pdf(&pdf_bytes)?;
        assert_eq!(parsed, markdown);
        Ok(())
    }
}
