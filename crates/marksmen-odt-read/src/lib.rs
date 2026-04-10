//! Reader for OpenDocument Format (`.odt`) archives.
//!
//! Reconstructs a structured Markdown parsing stream by unpacking the ODF Zip container
//! and executing a semantic traversal over `content.xml`.

use anyhow::{Context, Result};
use std::io::{Cursor, Read};
use quick_xml::events::Event;
use quick_xml::Reader;

/// Analytically extracts `.odt` binary payloads into a mathematically equivalent Markdown string.
/// Traverses `content.xml` nodes such as `<text:p>`, `<text:h>`, and `<text:span>` to reconstruct
/// standard Markdown block semantics and semantic text bounds (`S_Bold` -> `**`, `S_Italic` -> `*`).
pub fn parse_odt(bytes: &[u8]) -> Result<String> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .context("Failed to parse bytes as a ZIP ODT archive")?;

    let mut doc_xml = String::new();
    {
        let mut file = archive
            .by_name("content.xml")
            .context("Missing content.xml in ODT archive")?;
        file.read_to_string(&mut doc_xml)
            .context("Failed to read content.xml")?;
    }

    let mut reader = Reader::from_str(&doc_xml);
    reader.config_mut().trim_text(false);
    
    let mut output = String::new();
    
    let mut in_p = false;
    let mut in_span = false;
    let mut is_bold = false;
    let mut is_italic = false;
    let mut is_code = false;
    let mut is_underline = false;
    let mut is_sub = false;
    let mut is_sup = false;
    let mut is_display_math = false;
    let mut is_inline_math = false;
    let mut in_hidden_meta = false;
    let mut hidden_meta_text = String::new();
    
    let mut in_tbl = 0;
    let mut tr_count = 0;
    let mut tc_count = 0;
    let mut tc_alignments: Vec<u8> = Vec::new();
    let mut current_tc_alignment = 0;
    
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"text:p" | b"text:hidden-paragraph" => {
                        in_p = true;
                        is_display_math = false;
                        is_inline_math = false;
                        in_hidden_meta = false;
                        hidden_meta_text.clear();
                        let mut is_quote_paragraph = false;
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"text:style-name" {
                                    if a.value.as_ref() == b"P_Rule" {
                                        is_display_math = true;
                                    } else if a.value.as_ref() == b"P_DisplayMath" {
                                        is_display_math = true;
                                    } else if a.value.as_ref() == b"P_Right" {
                                        current_tc_alignment = 2;
                                    } else if a.value.as_ref() == b"P_Quote" {
                                        is_quote_paragraph = true;
                                    } else if a.value.as_ref() == b"P_HiddenMeta" {
                                        in_hidden_meta = true;
                                    }
                                }
                            }
                        }
                        if in_hidden_meta {
                            continue;
                        }
                        if in_tbl == 0 {
                            if output.len() > 0 && !output.ends_with("\n\n") && !output.ends_with("\n") && !output.ends_with("- ") {
                                output.push_str("\n\n");
                            }
                            if is_quote_paragraph {
                                output.push_str("> ");
                            }
                        } else {
                            if !output.ends_with("| ") && !output.ends_with(" ") {
                                output.push_str(" ");
                            }
                        }
                    }
                    b"text:h" => {
                        in_p = true; // treat heading as a block
                        let mut heading_level = 1u8;
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"text:outline-level" {
                                    heading_level = String::from_utf8_lossy(a.value.as_ref()).parse::<u8>().unwrap_or(1).clamp(1, 6);
                                }
                            }
                        }
                        output.push_str("\n\n");
                        output.push_str(&"#".repeat(heading_level as usize));
                        output.push(' ');
                    }
                    b"table:table" => {
                        in_tbl += 1;
                        tr_count = 0;
                        if output.len() > 0 && !output.ends_with("\n\n") { output.push_str("\n\n"); }
                    }
                    b"table:table-row" | b"table:table-header-rows" => {
                        if output.len() > 0 && !output.ends_with("\n") && !output.ends_with("\n\n") {
                            output.push_str("\n");
                        }
                        output.push_str("| ");
                        tc_count = 0;
                        tc_alignments.clear();
                    }
                    b"table:table-cell" => {
                        tc_count += 1;
                        current_tc_alignment = 0;
                    }
                    b"text:list" => {
                        if output.len() > 0 && !output.ends_with("\n\n") { output.push_str("\n\n"); }
                    }
                    b"text:list-item" => {
                        if output.len() > 0 && !output.ends_with("\n") { output.push_str("\n"); }
                        output.push_str("- ");
                    }
                    b"text:span" => {
                        in_span = true;
                        // Determine styling from text:style-name
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"text:style-name" {
                                    match a.value.as_ref() {
                                        b"S_Bold" => is_bold = true,
                                        b"S_Italic" => is_italic = true,
                                        b"S_MathInline" => is_inline_math = true,
                                        b"S_Code" => is_code = true,
                                        b"S_Underline" => is_underline = true,
                                        b"S_Sub" => is_sub = true,
                                        b"S_Sup" => is_sup = true,
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    b"text:line-break" => output.push_str("\n"),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"text:p" | b"text:h" | b"text:hidden-paragraph" => {
                    if in_hidden_meta {
                        let meta = hidden_meta_text.trim();
                        if !meta.is_empty() {
                            if output.ends_with("![Image]()") {
                                output.truncate(output.len() - "![Image]()".len());
                            }
                            if !output.ends_with("\n\n") && !output.is_empty() {
                                output.push_str("\n\n");
                            }
                            output.push_str(meta);
                        }
                        in_hidden_meta = false;
                        hidden_meta_text.clear();
                    }
                    in_p = false;
                }
                b"table:table-cell" => {
                    tc_alignments.push(current_tc_alignment);
                    output.push_str(" | ");
                }
                b"table:table-row" | b"table:table-header-rows" => {
                        output.push_str("\n");
                        tr_count += 1;
                        if tr_count == 1 {
                            output.push_str("|");
                            for i in 0..tc_count {
                                let align = tc_alignments.get(i).copied().unwrap_or(0);
                                match align {
                                    1 => output.push_str(" :---: |"),
                                    2 => output.push_str(" ---: |"),
                                    _ => output.push_str(" :--- |"),
                                }
                            }
                            output.push_str("\n");
                        }
                    }
                b"table:table" => {
                    in_tbl -= 1;
                    output.push_str("\n");
                }
                b"text:span" => {
                    in_span = false;
                    is_bold = false;
                    is_italic = false;
                    is_inline_math = false;
                    is_code = false;
                    is_underline = false;
                    is_sub = false;
                    is_sup = false;
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default();
                if text.trim().is_empty() {
                    continue;
                }
                if in_hidden_meta {
                    hidden_meta_text.push_str(&text);
                    continue;
                }
                if !text.is_empty() {
                    let mut formatted = text.to_string();
                    if formatted == "---" && is_display_math {
                        // This is an actual horizontal rule, keep it
                        output.push_str("---");
                        continue;
                    }
                    if is_display_math {
                        formatted = format!("$$\n{}\n$$", formatted);
                    } else if is_inline_math {
                        formatted = format!("${}$", formatted.trim());
                    } else if in_span || in_p {
                        let lead_chars = formatted.len() - formatted.trim_start_matches(' ').len();
                        let trail_chars = formatted.len() - formatted.trim_end_matches(' ').len();
                        
                        let mut core_text = formatted.trim().to_string();
                        if !core_text.is_empty() {
                            if is_code { core_text = format!("`{}`", core_text); }
                            if is_bold { core_text = format!("**{}**", core_text); }
                            if is_italic {
                                if !core_text.contains("$") { // Only wrap if it's not pre-wrapped math
                                    core_text = format!("*{}*", core_text);
                                }
                            }
                            if is_underline { core_text = format!("<u>{}</u>", core_text); }
                            if is_sub { core_text = format!("<sub>{}</sub>", core_text); }
                            if is_sup { core_text = format!("<sup>{}</sup>", core_text); }
                        }
                        formatted = format!("{}{}{}", " ".repeat(lead_chars), core_text, " ".repeat(trail_chars));
                    }
                    if formatted.starts_with("[Figure: ") {
                        formatted = "![Image]()".to_string();
                    }
                    output.push_str(&formatted);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break, // Truncate cleanly on error
            _ => {}
        }
    }

    let mut cleaned = output.trim().to_string();
    
    let mut last = String::new();
    while last != cleaned {
        last = cleaned.clone();
        cleaned = cleaned.replace("* *", " ");
        cleaned = cleaned.replace("** **", " ");
        cleaned = cleaned.replace("_ _", " ");
        cleaned = cleaned.replace("~ ~", " ");
    }
    
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::parse_odt;
    use std::io::Write;

    fn synthetic_odt(content_xml: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut bytes);
            let mut writer = zip::ZipWriter::new(cursor);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file("content.xml", options).unwrap();
            writer.write_all(content_xml.as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        bytes
    }

    #[test]
    fn parses_heading_math_and_hidden_image_metadata() {
        let content_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
  xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:body>
    <office:text>
      <text:h text:outline-level="2">Section Title</text:h>
      <text:p text:style-name="P_DisplayMath">x + y</text:p>
      <text:p>[Figure: ./architecture.svg]</text:p>
      <text:p text:style-name="P_HiddenMeta">![Architecture Diagram](./architecture.svg)</text:p>
      <text:p><text:span text:style-name="S_MathInline">a^2+b^2</text:span></text:p>
    </office:text>
  </office:body>
</office:document-content>"#;

        let parsed = parse_odt(&synthetic_odt(content_xml)).unwrap();
        assert!(parsed.contains("## Section Title"));
        assert!(parsed.contains("$$\nx + y\n$$"));
        assert!(parsed.contains("![Architecture Diagram](./architecture.svg)"));
        assert!(parsed.contains("$a^2+b^2$"));
    }
}
