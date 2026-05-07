//! Demonstrates cyclic document generation and raw string re-extraction from
//! compiled binary artifacts back to Markdown strings.

use anyhow::Result;
use marksmen_core::config::frontmatter::parse_frontmatter;
use marksmen_core::parsing::parser::parse;
use marksmen_core::Config;
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let source_str = "## Roundtrip Sequence\nSimulating continuous extraction sequences for **OpenXML** and **ODF** arrays.";
    let (body, _) = parse_frontmatter(source_str)?;
    let config = Config::default();

    // 1. DOCX Roundtrip
    let events_docx = parse(body);
    let docx_bytes =
        marksmen_docx::translation::document::convert(&events_docx, &config, &root, None)?;
    let extracted_docx_md = marksmen_docx_read::parse_docx(&docx_bytes, None)?;

    println!("=== Extracted from DOCX Bytes ===");
    println!("{}\n", extracted_docx_md);

    // 2. ODT Roundtrip
    let events_odt = parse(body);
    let odt_bytes = marksmen_odt::translate_and_render(&events_odt, &config, &root)?;
    let extracted_odt_md = marksmen_odt_read::parse_odt(&odt_bytes, None)?;

    println!("=== Extracted from ODT Bytes ===");
    println!("{}\n", extracted_odt_md);

    Ok(())
}
