//! Demonstrates native cross-compilation from Markdown to binary document targets.

use anyhow::Result;
use marksmen_core::Config;
use marksmen_core::config::frontmatter::parse_frontmatter;
use marksmen_core::parsing::parser::parse;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<()> {
    let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    
    let markdown_source = "## 1. Document Export Demo\nGenerating **DOCX**, **ODT**, **PDF**, and **HTML** structures natively.";
    let (body, _) = parse_frontmatter(markdown_source)?;
    let config = Config::default();
    
    // 1. HTML5 Extraction
    let events_html = parse(body);
    let html_out = marksmen_html::convert(events_html, &config)?;
    fs::write(root.join("demo_export.html"), &html_out)?;
    println!("[+] Generated demo_export.html natively.");

    // 2. OpenXML DOCX Compilation
    let events_docx = parse(body);
    let docx_bytes = marksmen_docx::translation::document::convert(events_docx, &config, &root)?;
    fs::write(root.join("demo_export.docx"), &docx_bytes)?;
    println!("[+] Generated demo_export.docx ({} bytes).", docx_bytes.len());

    // 3. OpenDocument ODT Compilation
    let events_odt = parse(body);
    let odt_bytes = marksmen_odt::translate_and_render(&events_odt, &config, &root)?;
    fs::write(root.join("demo_export.odt"), &odt_bytes)?;
    println!("[+] Generated demo_export.odt ({} bytes).", odt_bytes.len());

    // 4. Typst PDF Execution
    // marksmen_pdf consumes the raw markdown string statically.
    let pdf_bytes = marksmen_pdf::convert(markdown_source, &config, Some(root.clone()))?;
    fs::write(root.join("demo_export.pdf"), &pdf_bytes)?;
    println!("[+] Generated demo_export.pdf ({} bytes).", pdf_bytes.len());

    Ok(())
}
