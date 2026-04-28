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
pub fn convert(
    markdown: &str,
    config: &Config,
    base_path: Option<std::path::PathBuf>,
) -> Result<Vec<u8>> {
    // Step 1: Parse front-matter and merge with provided config.
    let (body, front_matter_config) =
        marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
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

    tracing::info!(
        pdf_bytes_len = pdf_bytes.len(),
        "PDF generated successfully"
    );

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
    // Parse both wrapped <mark> tags (with inner text) and empty orphan tags.
    let pages = document.get_pages();
    let total_pages = pages.len();
    if total_pages > 0 {
        let md_len = markdown.len().max(1);

        #[derive(Debug)]
        struct CommentInfo {
            byte_offset: usize,
            author: String,
            content: String,
            /// Parsed subtype from class attribute: "comment", "comment highlight", etc.
            subtype_class: String,
            /// Inner text of the <mark> tag, if any.
            inner_text: Option<String>,
        }
        let mut comments: Vec<CommentInfo> = Vec::new();

        let mut search_idx = 0;
        while let Some(start_idx) = markdown[search_idx..].find("<mark") {
            let actual_start = search_idx + start_idx;
            // Find the matching </mark> closing tag.
            if let Some(end_idx) = markdown[actual_start..].find("</mark>") {
                let full_tag = &markdown[actual_start..actual_start + end_idx + 7];
                // Extract class attribute.
                let class = extract_attr(full_tag, "class").unwrap_or_default();
                if !class.contains("comment")
                    && !class.contains("highlight")
                    && !class.contains("caret")
                    && !class.contains("strikeout")
                {
                    search_idx = actual_start + 1;
                    continue;
                }
                let author =
                    extract_attr(full_tag, "data-author").unwrap_or_else(|| "Author".to_string());
                let content = extract_attr(full_tag, "data-content").unwrap_or_default();
                let subtype =
                    extract_attr(full_tag, "data-subtype").unwrap_or_else(|| class.clone());
                // Extract inner text between the opening tag and </mark>.
                let inner_text = if let Some(gt) = full_tag.find('>') {
                    let inner = &full_tag[gt + 1..full_tag.len() - 7]; // strip after '>' and '</mark>'
                    if inner.is_empty() {
                        None
                    } else {
                        Some(inner.to_string())
                    }
                } else {
                    None
                };
                comments.push(CommentInfo {
                    byte_offset: actual_start,
                    author,
                    content,
                    subtype_class: subtype,
                    inner_text,
                });
                search_idx = actual_start + end_idx + 7;
            } else {
                break;
            }
        }

        let mut page_annots: std::collections::BTreeMap<u32, Vec<lopdf::ObjectId>> =
            std::collections::BTreeMap::new();

        for comment in &comments {
            let fraction = comment.byte_offset as f64 / md_len as f64;
            let page_num =
                ((fraction * total_pages as f64).floor() as u32).clamp(1, total_pages as u32);

            let count = page_annots.entry(page_num).or_default().len();
            let y_offset = 750.0_f32 - (count as f32 * 30.0);

            let pdf_subtype: &[u8] = if comment.subtype_class.contains("highlight") {
                b"Highlight"
            } else if comment.subtype_class.contains("caret") {
                b"Caret"
            } else if comment.subtype_class.contains("strikeout") {
                b"StrikeOut"
            } else {
                b"Text"
            };

            let mut annot_dict = Dictionary::new();
            annot_dict.set("Type", Object::Name(b"Annot".to_vec()));
            annot_dict.set("Subtype", Object::Name(pdf_subtype.to_vec()));
            annot_dict.set("T", Object::string_literal(comment.author.clone()));
            // If content is empty but inner_text exists, use inner_text as fallback.
            let display_content = if comment.content.is_empty() {
                comment.inner_text.clone().unwrap_or_default()
            } else {
                comment.content.clone()
            };
            annot_dict.set("Contents", Object::string_literal(display_content));
            annot_dict.set("MarksmenOrigin", Object::Boolean(true));
            annot_dict.set(
                "Rect",
                Object::Array(vec![
                    Object::Real(10.0),
                    Object::Real(y_offset - 20.0),
                    Object::Real(30.0),
                    Object::Real(y_offset),
                ]),
            );

            let annot_id = document.add_object(annot_dict);
            page_annots.entry(page_num).or_default().push(annot_id);
        }

        for (page_num, annot_obj_ids) in page_annots {
            if let Some(&page_obj_id) = pages.get(&page_num) {
                let annot_refs: Vec<Object> = annot_obj_ids
                    .iter()
                    .map(|id| Object::Reference(*id))
                    .collect();
                if let Ok(page_dict) = document
                    .get_object_mut(page_obj_id)
                    .and_then(|obj| obj.as_dict_mut())
                {
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
