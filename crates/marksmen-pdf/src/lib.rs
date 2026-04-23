pub mod rendering;

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
    let typst_source = marksmen_typst::translator::translate(events, &merged_config)?;

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
    
    // --- Phase 2: Native PDF Comment Injection ---
    // Distribute annotations across pages proportionally based on byte offset.
    let pages = document.get_pages();
    let total_pages = pages.len();
    if total_pages > 0 {
        let md_len = markdown.len().max(1);
        
        // Collect all comment positions and metadata.
        struct CommentInfo {
            byte_offset: usize,
            author: String,
            content: String,
        }
        let mut comments: Vec<CommentInfo> = Vec::new();
        
        let mut search_idx = 0;
        while let Some(start_idx) = markdown[search_idx..].find("<mark class=\"comment\"") {
            let actual_start = search_idx + start_idx;
            if let Some(end_idx) = markdown[actual_start..].find("</mark>") {
                let tag = &markdown[actual_start..actual_start + end_idx + 7];
                let author = extract_attr(tag, "data-author").unwrap_or_else(|| "Author".to_string());
                let content = extract_attr(tag, "data-content").unwrap_or_default();
                comments.push(CommentInfo { byte_offset: actual_start, author, content });
                search_idx = actual_start + end_idx + 7;
            } else {
                break;
            }
        }
        
        // Group annotations by target page.
        let mut page_annots: std::collections::BTreeMap<u32, Vec<lopdf::ObjectId>> = std::collections::BTreeMap::new();
        
        for comment in &comments {
            // Map byte offset to page number (1-indexed).
            let fraction = comment.byte_offset as f64 / md_len as f64;
            let page_num = ((fraction * total_pages as f64).floor() as u32).clamp(1, total_pages as u32);
            
            // Compute vertical offset per page: stack from top.
            let count = page_annots.entry(page_num).or_default().len();
            let y_offset = 750.0_f32 - (count as f32 * 30.0);
            
            let mut annot_dict = Dictionary::new();
            annot_dict.set("Type", Object::Name(b"Annot".to_vec()));
            annot_dict.set("Subtype", Object::Name(b"Text".to_vec()));
            annot_dict.set("T", Object::string_literal(comment.author.clone()));
            annot_dict.set("Contents", Object::string_literal(comment.content.clone()));
            annot_dict.set("MarksmenOrigin", Object::Boolean(true));
            annot_dict.set("Rect", Object::Array(vec![
                Object::Real(10.0),
                Object::Real(y_offset - 20.0),
                Object::Real(30.0),
                Object::Real(y_offset),
            ]));
            
            let annot_id = document.add_object(annot_dict);
            page_annots.entry(page_num).or_default().push(annot_id);
        }
        
        // Attach annotations to their respective pages.
        for (page_num, annot_obj_ids) in page_annots {
            if let Some(&page_obj_id) = pages.get(&page_num) {
                let annot_refs: Vec<Object> = annot_obj_ids.iter().map(|id| Object::Reference(*id)).collect();
                if let Ok(page_dict) = document.get_object_mut(page_obj_id).and_then(|obj| obj.as_dict_mut()) {
                    if let Ok(existing_annots) = page_dict.get_mut(b"Annots") {
                        if let Object::Array(arr) = existing_annots {
                            arr.extend(annot_refs);
                        }
                    } else {
                        page_dict.set("Annots", Object::Array(annot_refs));
                    }
                }
            }
        }
    }

    document
        .save_to(&mut out)
        .context("Failed to save PDF with embedded roundtrip metadata")?;
    Ok(out)
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    if let Some(start) = tag.find(&needle) {
        if let Some(end) = tag[start + needle.len()..].find('"') {
            return Some(tag[start + needle.len()..start + needle.len() + end].to_string());
        }
    }
    None
}
