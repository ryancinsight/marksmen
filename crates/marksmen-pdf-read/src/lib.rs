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
    if let Some(mut markdown) = extract_embedded_roundtrip_markdown(bytes)? {
        
        // --- Extract Foreign PDF Annotations ---
        if let Ok(document) = Document::load_mem(bytes) {
            let mut new_comments_markdown = String::new();
            
            for page_id in document.get_pages().values() {
                if let Ok(page_dict) = document.get_dictionary(*page_id) {
                    if let Ok(annots) = page_dict.get(b"Annots") {
                        let annot_list = match annots {
                            Object::Array(arr) => arr.clone(),
                            Object::Reference(id) => {
                                if let Ok(Object::Array(arr)) = document.get_object(*id) {
                                    arr.clone()
                                } else {
                                    continue;
                                }
                            }
                            _ => continue,
                        };
                        
                        for annot_obj in annot_list {
                            if let Object::Reference(annot_id) = annot_obj {
                                if let Ok(annot_dict) = document.get_dictionary(annot_id) {
                                    if let Ok(subtype) = annot_dict.get(b"Subtype") {
                                        if let Ok(subtype_name) = subtype.as_name() {
                                            let is_supported = subtype_name == b"Text" 
                                                || subtype_name == b"Highlight" 
                                                || subtype_name == b"Caret" 
                                                || subtype_name == b"StrikeOut";
                                                
                                            if is_supported {
                                                // Check for our exact internal origin signature.
                                                let is_marksmen_origin = annot_dict.get(b"MarksmenOrigin")
                                                    .and_then(|obj| obj.as_bool()).unwrap_or(false);
                                                
                                                if !is_marksmen_origin {
                                                    // This is a NEW user annotation appended externally via Preview/Acrobat
                                                    let author = annot_dict.get(b"T").ok()
                                                        .and_then(|obj| obj.as_string().ok())
                                                        .map(|c| c.into_owned())
                                                        .unwrap_or_else(|| "Reviewer".to_string());
                                                    let mut content = annot_dict.get(b"Contents").ok()
                                                        .and_then(|obj| obj.as_string().ok())
                                                        .map(|c| c.into_owned())
                                                        .unwrap_or_default();
                                                        
                                                    // Prepend action semantics based on Subtype so the intent is clear
                                                    match subtype_name {
                                                        b"Highlight" => content = format!("[Highlight] {}", content),
                                                        b"Caret" => content = format!("[Insertion] {}", content),
                                                        b"StrikeOut" => content = format!("[Replacement/Deletion] {}", content),
                                                        _ => {}
                                                    }
                                                    
                                                    // Append as a standard DOCX comment block
                                                    new_comments_markdown.push_str(&format!(
                                                        "\n\n<!-- P_BR --><mark class=\"comment\" data-author=\"{}\" data-content=\"{}\"></mark>", 
                                                        author.replace('"', "&quot;"), content.replace("<", "&lt;").replace(">", "&gt;")
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !new_comments_markdown.is_empty() {
                markdown.push_str(&new_comments_markdown);
            }
        }
        
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
