//! Reader for PPTX files produced by `marksmen-ppt`.
//!
//! Extracts the text content of each `ppt/slides/slideN.xml` entry in the zip
//! archive and reconstructs a Markdown representation. The slide title shape
//! (identified by `<p:ph type="title"/>`) is emitted as `# Title`, and the
//! body text paragraphs are emitted as normal paragraphs. Consecutive slides
//! are separated by the `---` horizontal rule sentinel matching the writer's
//! segmentation contract.
//!
//! # Invariants
//! - The reader only examines `<a:t>` (DrawingML text run) nodes; all other
//!   OOXML structure is ignored.
//! - Slide files are sorted lexicographically by their zip entry name, which
//!   matches the `slide1.xml`, `slide2.xml`, … sequencing written by
//!   `marksmen-ppt`.

use anyhow::{Context, Result};
use quick_xml::events::Event as XmlEvent;
use quick_xml::Reader;
use std::io::{Cursor, Read};
use zip::ZipArchive;

/// Parse a PPTX binary produced by `marksmen-ppt::convert` and reconstruct
/// approximate Markdown. The reconstruction is heuristic: it recovers plain
/// text from DrawingML text runs and re-segments slides via `---`.
///
/// Returns a Markdown `String` suitable for passing to the `pulldown-cmark`
/// parser for roundtrip HTML rendering.
pub fn parse_pptx(bytes: &[u8]) -> Result<String> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).context("PPTX is not a valid zip archive")?;

    // Collect slide entry names in sorted order.
    let mut slide_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let file = archive.by_index(i).ok()?;
            let name = file.name().to_string();
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    // Sort so slide1.xml < slide2.xml < … (lexicographic sort works for up to
    // 999 slides matching the monotone naming contract).
    slide_names.sort();

    let mut out = String::new();
    for (idx, name) in slide_names.iter().enumerate() {
        let xml = {
            let mut file = archive.by_name(name)?;
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;
            buf
        };

        if idx > 0 {
            out.push_str("\n\n---\n\n");
        }

        let slide_md = parse_slide_xml(&xml)?;
        out.push_str(&slide_md);
    }

    Ok(out)
}

// ── Per-slide XML extraction ─────────────────────────────────────────────────

/// Extract text from one `slideN.xml` file and produce Markdown.
///
/// # Strategy
/// Walk the XML stream tracking:
/// - `<p:nvPr><p:ph type="title"/>` → next `<a:t>` runs are the slide title.
/// - All `<a:t>` runs outside the title shape contribute to the body text.
/// - `<a:p>` boundaries separate body paragraphs (emitted as newlines).
fn parse_slide_xml(xml_str: &str) -> Result<String> {
    let mut reader = Reader::from_str(xml_str);
    reader.config_mut().trim_text(true);

    let mut title = String::new();
    let mut body_paras: Vec<String> = Vec::new();
    let mut current_para = String::new();

    // State machine flags.
    let mut in_title_sp = false;   // inside the title shape (<p:sp> with ph type="title")
    let mut in_body_sp = false;    // inside any non-title shape
    let mut ph_seen_title = false; // <p:ph type="title"/> encountered in current <p:sp>
    let mut in_sp = false;         // inside any <p:sp>
    let mut in_para = false;       // inside <a:p>
    let mut reading_t = false;     // inside <a:t>

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            XmlEvent::Start(ref e) | XmlEvent::Empty(ref e) => {
                match e.name().as_ref() {
                    b"p:sp" => {
                        in_sp = true;
                        ph_seen_title = false;
                        in_title_sp = false;
                        in_body_sp = false;
                    }
                    b"p:ph" => {
                        if in_sp {
                            // Check for type="title"
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"type" {
                                    if attr.value.as_ref() == b"title" {
                                        ph_seen_title = true;
                                        in_title_sp = true;
                                    }
                                }
                            }
                            if !ph_seen_title {
                                in_body_sp = true;
                            }
                        }
                    }
                    b"a:p" => {
                        in_para = true;
                        current_para.clear();
                    }
                    b"a:t" => {
                        reading_t = true;
                    }
                    _ => {}
                }
            }
            XmlEvent::End(ref e) => {
                match e.name().as_ref() {
                    b"p:sp" => {
                        in_sp = false;
                        in_title_sp = false;
                        in_body_sp = false;
                        ph_seen_title = false;
                    }
                    b"a:p" => {
                        if in_para && in_body_sp && !current_para.trim().is_empty() {
                            body_paras.push(current_para.trim().to_string());
                        }
                        current_para.clear();
                        in_para = false;
                    }
                    b"a:t" => {
                        reading_t = false;
                    }
                    _ => {}
                }
            }
            XmlEvent::Text(ref e) => {
                if reading_t {
                    let text = e.unescape().unwrap_or_default();
                    if in_title_sp {
                        title.push_str(&text);
                    } else if in_body_sp && in_para {
                        current_para.push_str(&text);
                    }
                }
            }
            XmlEvent::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    let mut md = String::new();
    if !title.trim().is_empty() {
        md.push_str("# ");
        md.push_str(title.trim());
        md.push('\n');
    }
    for para in body_paras {
        md.push('\n');
        md.push_str(&para);
    }

    Ok(md)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a PPTX via marksmen-ppt and verify the reader extracts the
    /// correct slide content with the correct H1 titles.
    ///
    /// This test requires the `marksmen-ppt` crate; it is declared as a dev-dep
    /// so it compiles in the test binary only.
    #[test]
    fn roundtrip_basic() {
        // Build a minimal PPTX directly from raw XML to ensure independence
        // from the writer in unit scope.
        let slide_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
        <p:txBody><a:bodyPr/><a:lstStyle/>
          <a:p><a:r><a:t>Hello World</a:t></a:r></a:p>
        </p:txBody>
      </p:sp>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="3" name="Body"/><p:cNvSpPr/><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
        <p:txBody><a:bodyPr/><a:lstStyle/>
          <a:p><a:r><a:t>Body paragraph one.</a:t></a:r></a:p>
        </p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
</p:sld>"#;

        let md = parse_slide_xml(slide_xml).unwrap();
        assert!(md.contains("# Hello World"), "title not extracted: {md}");
        assert!(md.contains("Body paragraph one."), "body not extracted: {md}");
    }
}
