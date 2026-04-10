//! PDF extraction and text reconstruction for marksmen round-trip validation.
//!
//! Provides text boundary extraction leveraging the `pdf-extract` crate.

use anyhow::{Context, Result};

/// Parses raw PDF bytes and extracts text geometries into a single concatenated string.
///
/// This serves directly as a structural validator against compiled ASTs in the round-trip suite.
pub fn parse_pdf(bytes: &[u8]) -> Result<String> {
    let text = pdf_extract::extract_text_from_mem(bytes)
        .context("Failed to extract text structures from PDF buffer")?;
    Ok(text)
}
