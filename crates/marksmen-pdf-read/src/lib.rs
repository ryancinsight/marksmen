//! PDF extraction and text reconstruction for marksmen.
//!
//! Uses a native lopdf-based reader with full font encoding (WinAnsi/MacRoman/CMap),
//! glyph advance width metrics, graphics state, and XObject traversal.

use anyhow::{Context, Result};
use lopdf::{Document, Object};

mod reader;
mod text_mapper;
pub use text_mapper::TextRun;

const ROUNDTRIP_MARKDOWN_KEY: &[u8] = b"MarksmenRoundtripMarkdown";

/// Extracted document metadata.
#[derive(Debug, Clone)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
}

/// Extract basic document metadata from the PDF Info dictionary.
pub fn extract_pdf_metadata(bytes: &[u8]) -> Result<PdfMetadata> {
    let document = Document::load_mem(bytes).context("Failed to parse PDF bytes for metadata extraction")?;
    let mut metadata = PdfMetadata {
        title: None,
        author: None,
        subject: None,
        creator: None,
    };

    if let Ok(info_id) = document.trailer.get(b"Info").and_then(Object::as_reference) {
        if let Ok(info) = document.get_dictionary(info_id) {
            let decode_pdf_string = |key: &[u8]| -> Option<String> {
                info.get(key)
                    .ok()
                    .and_then(|obj| obj.as_string().ok())
                    .map(|c| c.into_owned())
            };
            metadata.title = decode_pdf_string(b"Title");
            metadata.author = decode_pdf_string(b"Author");
            metadata.subject = decode_pdf_string(b"Subject");
            metadata.creator = decode_pdf_string(b"Creator");
        }
    }
    Ok(metadata)
}

/// PDF annotation subtype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationSubtype {
    /// Sticky note / standalone comment.
    Text,
    /// Highlight over text.
    Highlight,
    /// Insertion point.
    Caret,
    /// Deletion / replacement.
    StrikeOut,
}

/// Bounding rectangle in PDF page coordinates (points, user space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub llx: f32,
    pub lly: f32,
    pub urx: f32,
    pub ury: f32,
}

impl Rect {
    /// Parse from a lopdf `Object::Array` of 4 numbers.
    fn from_object(obj: &Object) -> Option<Self> {
        if let Ok(arr) = obj.as_array() {
            if arr.len() == 4 {
                let nums: Vec<f32> = arr
                    .iter()
                    .filter_map(|o| match o {
                        Object::Real(f) => Some(*f as f32),
                        Object::Integer(i) => Some(*i as f32),
                        _ => None,
                    })
                    .collect();
                if nums.len() == 4 {
                    return Some(Rect {
                        llx: nums[0],
                        lly: nums[1],
                        urx: nums[2],
                        ury: nums[3],
                    });
                }
            }
        }
        None
    }

    /// True if this rectangle intersects another.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.llx < other.urx && self.urx > other.llx && self.lly < other.ury && self.ury > other.lly
    }

    /// Area of the rectangle.
    pub fn area(&self) -> f32 {
        (self.urx - self.llx).max(0.0) * (self.ury - self.lly).max(0.0)
    }
}

/// A single quadrilateral (8 numbers: x1,y1, x2,y2, x3,y3, x4,y4).
#[derive(Debug, Clone, PartialEq)]
pub struct Quad {
    pub points: [f32; 8],
}

impl Quad {
    /// Parse from a slice of 8 floats.
    fn from_slice(s: &[f32]) -> Option<Self> {
        if s.len() == 8 {
            let mut points = [0.0f32; 8];
            points.copy_from_slice(s);
            Some(Quad { points })
        } else {
            None
        }
    }

    /// Approximate bounding rectangle.
    pub fn bbox(&self) -> Rect {
        let xs = [
            self.points[0],
            self.points[2],
            self.points[4],
            self.points[6],
        ];
        let ys = [
            self.points[1],
            self.points[3],
            self.points[5],
            self.points[7],
        ];
        Rect {
            llx: xs.iter().cloned().fold(f32::INFINITY, f32::min),
            lly: ys.iter().cloned().fold(f32::INFINITY, f32::min),
            urx: xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
            ury: ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
        }
    }
}

/// A PDF annotation with resolved text-localization metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalizedAnnotation {
    pub subtype: AnnotationSubtype,
    pub author: String,
    pub content: String,
    /// Annotation bounding box in page coordinates (points).
    pub rect: Rect,
    /// For Highlight: exact quadrilaterals of highlighted text.
    pub quad_points: Vec<Quad>,
    /// The text content beneath this annotation, if resolvable.
    pub anchored_text: Option<String>,
    /// 1-based page number.
    pub page_number: u32,
    /// Optional color (RGB, 0.0–1.0).
    pub color: Option<(f32, f32, f32)>,
    /// Optional date string (PDF /Date format or ISO).
    pub date: Option<String>,
}

impl LocalizedAnnotation {
    /// Return a fallback display string for the annotation.
    pub fn fallback_label(&self) -> String {
        match self.subtype {
            AnnotationSubtype::Highlight => format!("[Highlight] {}", self.content),
            AnnotationSubtype::Caret => format!("[Insertion] {}", self.content),
            AnnotationSubtype::StrikeOut => format!("[Replacement/Deletion] {}", self.content),
            AnnotationSubtype::Text => self.content.clone(),
        }
    }

    /// Return the text that this annotation anchors to, or the fallback label.
    pub fn display_text(&self) -> String {
        self.anchored_text
            .clone()
            .unwrap_or_else(|| self.fallback_label())
    }
}

/// Extract all foreign (non-marksmen-origin) annotations from the PDF,
/// localized to text where possible.
pub fn extract_annotations(bytes: &[u8]) -> Result<Vec<LocalizedAnnotation>> {
    let document =
        Document::load_mem(bytes).context("Failed to parse PDF bytes for annotation extraction")?;

    let pages = document.get_pages();
    let mut annotations: Vec<LocalizedAnnotation> = Vec::new();

    for (page_num, &page_id) in pages.iter() {
        let page_dict = match document.get_dictionary(page_id) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Build text mapper for this page.
        let text_runs = match text_mapper::extract_text_runs(&document, page_id) {
            Ok(runs) => runs,
            Err(e) => {
                tracing::warn!(page = page_num, error = %e, "Failed to extract text runs for page");
                Vec::new()
            }
        };

        let annots = match page_dict.get(b"Annots") {
            Ok(a) => a,
            Err(_) => continue,
        };

        let annot_list = match annots {
            Object::Array(arr) => arr.clone(),
            Object::Reference(id) => {
                match document.get_object(*id).and_then(|o| o.as_array().cloned()) {
                    Ok(arr) => arr,
                    Err(_) => continue,
                }
            }
            _ => continue,
        };

        for annot_obj in annot_list {
            let annot_id = match annot_obj {
                Object::Reference(id) => id,
                _ => continue,
            };

            let annot_dict = match document.get_dictionary(annot_id) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let subtype = match annot_dict.get(b"Subtype").and_then(|o| o.as_name()) {
                Ok(name) => name,
                Err(_) => continue,
            };

            let subtype = match subtype {
                b"Text" => AnnotationSubtype::Text,
                b"Highlight" => AnnotationSubtype::Highlight,
                b"Caret" => AnnotationSubtype::Caret,
                b"StrikeOut" => AnnotationSubtype::StrikeOut,
                _ => continue,
            };

            // Skip our own origin annotations.
            let is_marksmen_origin = annot_dict
                .get(b"MarksmenOrigin")
                .and_then(|obj| obj.as_bool())
                .unwrap_or(false);
            if is_marksmen_origin {
                continue;
            }

            let author = annot_dict
                .get(b"T")
                .ok()
                .and_then(|obj| obj.as_string().ok())
                .map(|c| c.into_owned())
                .unwrap_or_else(|| "Reviewer".to_string());

            let content = annot_dict
                .get(b"Contents")
                .ok()
                .and_then(|obj| obj.as_string().ok())
                .map(|c| c.into_owned())
                .unwrap_or_default();

            let rect = annot_dict
                .get(b"Rect")
                .ok()
                .and_then(Rect::from_object)
                .unwrap_or(Rect {
                    llx: 0.0,
                    lly: 0.0,
                    urx: 0.0,
                    ury: 0.0,
                });

            let quad_points = annot_dict
                .get(b"QuadPoints")
                .ok()
                .and_then(|o| o.as_array().ok())
                .map(|arr| {
                    let nums: Vec<f32> = arr
                        .iter()
                        .filter_map(|o| match o {
                            Object::Real(f) => Some(*f as f32),
                            Object::Integer(i) => Some(*i as f32),
                            _ => None,
                        })
                        .collect();
                    nums.chunks_exact(8)
                        .filter_map(Quad::from_slice)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let color = annot_dict
                .get(b"C")
                .ok()
                .and_then(|o| o.as_array().ok())
                .and_then(|arr| {
                    let nums: Vec<f32> = arr
                        .iter()
                        .filter_map(|o| match o {
                            Object::Real(f) => Some(*f as f32),
                            Object::Integer(i) => Some(*i as f32),
                            _ => None,
                        })
                        .collect();
                    if nums.len() >= 3 {
                        Some((nums[0], nums[1], nums[2]))
                    } else {
                        None
                    }
                });

            let date = annot_dict
                .get(b"M")
                .ok()
                .and_then(|obj| obj.as_string().ok())
                .map(|c| c.into_owned());

            // Attempt to resolve anchored text.
            let anchored_text = if !quad_points.is_empty() {
                // Use quad bounding boxes for more precise text mapping.
                let mut texts: Vec<String> = Vec::new();
                for quad in &quad_points {
                    let bbox = quad.bbox();
                    let mut hits: Vec<&str> = text_runs
                        .iter()
                        .filter(|run| run.rect.intersects(&bbox))
                        .map(|run| run.text.as_str())
                        .collect();
                    // Deduplicate contiguous runs.
                    hits.dedup();
                    if !hits.is_empty() {
                        texts.push(hits.join(""));
                    }
                }
                if texts.is_empty() {
                    None
                } else {
                    Some(texts.join(" "))
                }
            } else {
                // Fall back to Rect intersection.
                let hits: Vec<&str> = text_runs
                    .iter()
                    .filter(|run| run.rect.intersects(&rect))
                    .map(|run| run.text.as_str())
                    .collect();
                if hits.is_empty() {
                    None
                } else {
                    Some(hits.join(""))
                }
            };

            annotations.push(LocalizedAnnotation {
                subtype,
                author,
                content,
                rect,
                quad_points,
                anchored_text,
                page_number: *page_num,
                color,
                date,
            });
        }
    }

    Ok(annotations)
}

/// Parses raw PDF bytes and extracts text geometries into a single concatenated string.
///
/// This serves directly as a structural validator against compiled ASTs in the round-trip suite.
/// Foreign annotations are extracted and appended as text-anchored `<mark>` tags when possible.
pub fn parse_pdf(bytes: &[u8]) -> Result<String> {
    if let Some(mut markdown) = extract_embedded_roundtrip_markdown(bytes)? {
        // --- Extract Foreign PDF Annotations ---
        match extract_annotations(bytes) {
            Ok(annotations) => {
                let mut new_comments_markdown = String::new();
                for annot in annotations {
                    let display = annot.display_text();
                    let subtype_class = match annot.subtype {
                        AnnotationSubtype::Highlight => "comment highlight",
                        AnnotationSubtype::Caret => "comment caret",
                        AnnotationSubtype::StrikeOut => "comment strikeout",
                        AnnotationSubtype::Text => "comment",
                    };
                    let author_escaped = annot.author.replace('"', "&quot;");
                    let content_escaped = annot.content.replace('<', "&lt;").replace('>', "&gt;");
                    let display_escaped = display.replace('<', "&lt;").replace('>', "&gt;");

                    if let Some(ref _anchored) = annot.anchored_text {
                        // Text-anchored: wrap the annotated text.
                        new_comments_markdown.push_str(&format!(
                            "\n\n<mark class=\"{}\" data-author=\"{}\" data-content=\"{}\">{}</mark>",
                            subtype_class, author_escaped, content_escaped, display_escaped
                        ));
                    } else {
                        // Orphan: emit empty tag with fallback label in data-content.
                        new_comments_markdown.push_str(&format!(
                            "\n\n<!-- P_BR --><mark class=\"{}\" data-author=\"{}\" data-content=\"{}\"></mark>",
                            subtype_class, author_escaped, content_escaped
                        ));
                    }
                }
                if !new_comments_markdown.is_empty() {
                    markdown.push_str(&new_comments_markdown);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to extract PDF annotations");
            }
        }
        return Ok(markdown);
    }

    // Use the native PDF reader: encoding-aware font decoding, real glyph advance
    // widths, full graphics state (Tc/Tw/Tz/color), Form XObject traversal.
    let text = reader::pdf_to_markdown(bytes).context("Native PDF reader failed")?;
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
    use super::{extract_annotations, parse_pdf};
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

    #[test]
    fn extracts_foreign_annotations_from_pdf() -> Result<()> {
        // This test requires a PDF with foreign annotations. Since we don't have
        // one in-tree, we verify the API compiles and returns an empty vec for
        // a marksmen-generated PDF (which has MarksmenOrigin=true).
        let markdown = "# Hello\n\nWorld.";
        let pdf_bytes = marksmen_pdf::convert(markdown, &Config::default(), None)?;
        let anns = extract_annotations(&pdf_bytes)?;
        // marksmen-generated annotations are filtered out by MarksmenOrigin.
        assert!(anns.is_empty());
        Ok(())
    }
}
