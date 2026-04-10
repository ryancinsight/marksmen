pub mod rendering;
pub mod translation;

use anyhow::{Context, Result};
use lopdf::{Dictionary, Document, Object};
use marksmen_core::config::Config;

const ROUNDTRIP_MARKDOWN_KEY: &str = "MarksmenRoundtripMarkdown";

/// Convert a Markdown string to PDF bytes using the Typst translation and rendering engine.
///
/// This is the primary entry point for the PDF generation library. It:
/// 1. Parses front-matter configuration from the markdown
/// 2. Parses the markdown body into an event stream via `marksmen_core`
/// 3. Translates events to Typst markup
/// 4. Compiles Typst markup to a PDF document
/// 5. Exports the document as PDF bytes
pub fn convert(markdown: &str, config: &Config, base_path: Option<std::path::PathBuf>) -> Result<Vec<u8>> {
    // Step 1: Parse front-matter and merge with provided config.
    let (body, front_matter_config) = marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
    let merged_config = config.merge_frontmatter(&front_matter_config);

    // Step 2: Parse the markdown body into events.
    let events = marksmen_core::parsing::parser::parse(body);

    // Step 3: Translate events to Typst markup.
    let typst_source = translation::translator::translate(events, &merged_config)?;

    tracing::debug!(
        typst_source_len = typst_source.len(),
        "Generated Typst markup"
    );

    // Step 4 & 5: Compile Typst → PDF.
    let pdf_bytes = rendering::compiler::compile_to_pdf(&typst_source, &merged_config, base_path)?;
    let pdf_bytes = embed_roundtrip_markdown(&pdf_bytes, markdown)?;

    tracing::info!(pdf_bytes_len = pdf_bytes.len(), "PDF generated successfully");

    Ok(pdf_bytes)
}

fn embed_roundtrip_markdown(pdf_bytes: &[u8], markdown: &str) -> Result<Vec<u8>> {
    let mut document = Document::load_mem(pdf_bytes)
        .context("Failed to reload generated PDF for metadata embedding")?;

    let info_id = match document.trailer.get(b"Info").and_then(Object::as_reference) {
        Ok(id) => id,
        Err(_) => {
            let id = document.add_object(Dictionary::new());
            document.trailer.set("Info", id);
            id
        }
    };

    let info = document
        .get_dictionary_mut(info_id)
        .context("Failed to access PDF Info dictionary")?;
    info.set(ROUNDTRIP_MARKDOWN_KEY, Object::string_literal(markdown));

    let mut out = Vec::new();
    document
        .save_to(&mut out)
        .context("Failed to save PDF with embedded roundtrip metadata")?;
    Ok(out)
}
