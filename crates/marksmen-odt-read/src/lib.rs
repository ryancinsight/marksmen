//! Reader for OpenDocument Format (`.odt`) archives.
//!
//! Reconstructs a structured Markdown parsing stream by unpacking the ODF Zip container
//! and executing a semantic traversal over `content.xml`.

use anyhow::{Context, Result};
use marksmen_xml_read::Event;
use marksmen_xml_read::Reader;
use std::io::{Cursor, Read};

/// Analytically extracts `.odt` binary payloads into a mathematically equivalent Markdown string.
/// Traverses `content.xml` nodes such as `<text:p>`, `<text:h>`, and `<text:span>` to reconstruct
/// standard Markdown block semantics and semantic text bounds (`S_Bold` -> `**`, `S_Italic` -> `*`).
pub fn parse_odt(bytes: &[u8], media_out_dir: Option<&std::path::Path>) -> Result<String> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).context("Failed to parse bytes as a ZIP ODT archive")?;

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
    let mut is_strikethrough = false;
    let mut is_code = false;
    let mut is_underline = false;
    let mut is_sub = false;
    let mut is_sup = false;
    let mut is_display_math = false;
    let mut is_inline_math = false;
    let mut in_hidden_meta = false;
    let mut hidden_meta_text = String::new();
    let mut in_hidden_span_meta = false;
    let mut hidden_span_meta_text = String::new();
    // P_DisplayMath paragraphs may contain either draw:frame (MathML) paired with a
    // following P_HiddenMeta, or plain text that should be emitted directly as $$...$$.
    let mut in_display_math_para = false;
    let mut display_math_text = String::new();
    let mut link_href = String::new();

    let mut in_tbl = 0;
    let mut tr_count = 0;
    let mut tc_count = 0;
    let mut tc_alignments: Vec<u8> = Vec::new();
    let mut current_tc_alignment = 0;
    // List state: ordered flag and item counter per nesting level.
    // L_Numbered maps to ordered (1. 2. 3.), L_Bullet to unordered (-).
    let mut list_ordered_stack: Vec<bool> = Vec::new();
    let mut list_counter_stack: Vec<u32> = Vec::new();

    let mut tracked_changes_map: std::collections::HashMap<String, (String, String, String)> =
        std::collections::HashMap::new();
    let mut current_change_id = String::new();
    let mut current_change_type = String::new(); // "ins" or "del"
    let mut in_creator = false;
    let mut in_date = false;
    let mut current_creator = String::new();
    let mut current_date = String::new();

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
                        in_display_math_para = false;
                        hidden_meta_text.clear();
                        let mut is_quote_paragraph = false;
                        for attr in e.attributes() {
                            if let Ok(a) = attr
                                && a.key.as_ref() == b"text:style-name"
                            {
                                if a.value.as_ref() == b"P_Rule" {
                                    // Horizontal rule — not math.
                                } else if a.value.as_ref() == b"P_DisplayMath" {
                                    in_display_math_para = true;
                                } else if a.value.as_ref() == b"P_Right" {
                                    current_tc_alignment = 2;
                                } else if a.value.as_ref() == b"P_Center" {
                                    current_tc_alignment = 1;
                                } else if a.value.as_ref() == b"P_Left" {
                                    current_tc_alignment = 0;
                                } else if a.value.as_ref() == b"P_Quote" {
                                    is_quote_paragraph = true;
                                } else if a.value.as_ref() == b"P_HiddenMeta" {
                                    in_hidden_meta = true;
                                }
                            }
                        }
                        if in_hidden_meta {
                            continue;
                        }
                        if in_tbl == 0 {
                            if !list_ordered_stack.is_empty() {
                                // Inside a list item: text:p is the item body;
                                // do not insert paragraph break between the marker and body text.
                            } else if !output.is_empty()
                                && !output.ends_with("\n\n")
                                && !output.ends_with("\n")
                                && !output.ends_with("- ")
                            {
                                output.push_str("\n\n");
                            }
                            if is_quote_paragraph {
                                output.push_str("> ");
                            }
                        } else {
                            if !output.ends_with("| ") && !output.ends_with(" ") {
                                output.push(' ');
                            }
                        }
                    }
                    b"text:h" => {
                        in_p = true; // treat heading as a block
                        let mut heading_level = 1u8;
                        for attr in e.attributes() {
                            if let Ok(a) = attr
                                && a.key.as_ref() == b"text:outline-level"
                            {
                                heading_level = String::from_utf8_lossy(a.value.as_ref())
                                    .parse::<u8>()
                                    .unwrap_or(1)
                                    .clamp(1, 6);
                            }
                        }
                        output.push_str("\n\n");
                        output.push_str(&"#".repeat(heading_level as usize));
                        output.push(' ');
                    }
                    b"table:table" => {
                        in_tbl += 1;
                        tr_count = 0;
                        if !output.is_empty() && !output.ends_with("\n\n") {
                            output.push_str("\n\n");
                        }
                    }
                    b"table:table-row" | b"table:table-header-rows" => {
                        if !output.is_empty()
                            && !output.ends_with("\n")
                            && !output.ends_with("\n\n")
                        {
                            output.push('\n');
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
                        // Detect list style from text:style-name attribute.
                        let mut is_ordered = false;
                        for attr in e.attributes() {
                            if let Ok(a) = attr
                                && a.key.as_ref() == b"text:style-name"
                                && a.value.as_ref() == b"L_Numbered"
                            {
                                is_ordered = true;
                            }
                        }
                        list_ordered_stack.push(is_ordered);
                        list_counter_stack.push(0);
                        if !output.is_empty()
                            && !output.ends_with("\n\n")
                            && !output.ends_with("\n")
                        {
                            output.push('\n');
                        }
                    }
                    b"text:list-item" => {
                        let depth = list_ordered_stack.len().saturating_sub(1);
                        let indent = "    ".repeat(depth);
                        let is_ordered = list_ordered_stack.last().copied().unwrap_or(false);
                        if let Some(counter) = list_counter_stack.last_mut() {
                            *counter += 1;
                            if is_ordered {
                                if !output.is_empty() && !output.ends_with("\n") {
                                    output.push('\n');
                                }
                                output.push_str(&format!("{}{}. ", indent, counter));
                            } else {
                                if !output.is_empty() && !output.ends_with("\n") {
                                    output.push('\n');
                                }
                                output.push_str(&format!("{}- ", indent));
                            }
                        }
                    }
                    b"text:span" => {
                        in_span = true;
                        // Determine styling from text:style-name
                        for attr in e.attributes() {
                            if let Ok(a) = attr
                                && a.key.as_ref() == b"text:style-name"
                            {
                                match a.value.as_ref() {
                                    b"S_Bold" => is_bold = true,
                                    b"S_Italic" => is_italic = true,
                                    b"S_Strikethrough" => is_strikethrough = true,
                                    b"S_MathInline" => is_inline_math = true,
                                    b"S_Code" => is_code = true,
                                    b"S_Underline" => is_underline = true,
                                    b"S_Sub" => is_sub = true,
                                    b"S_Sup" => is_sup = true,
                                    b"S_HiddenMeta" => in_hidden_span_meta = true,
                                    _ => {}
                                }
                            }
                        }
                    }
                    b"text:line-break" => output.push('\n'),
                    b"text:a" => {
                        for attr in e.attributes() {
                            if let Ok(a) = attr
                                && a.key.as_ref() == b"xlink:href"
                            {
                                link_href = String::from_utf8_lossy(a.value.as_ref()).into_owned();
                            }
                        }
                        output.push('[');
                    }
                    b"text:changed-region" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"text:id" {
                                current_change_id =
                                    String::from_utf8_lossy(&attr.value).into_owned();
                            }
                        }
                    }
                    b"text:insertion" => {
                        current_change_type = "ins".to_string();
                    }
                    b"text:deletion" => {
                        current_change_type = "del".to_string();
                    }
                    b"dc:creator" => {
                        in_creator = true;
                        current_creator.clear();
                    }
                    b"dc:date" => {
                        in_date = true;
                        current_date.clear();
                    }
                    b"text:change-start" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"text:change-id" {
                                let cid = String::from_utf8_lossy(&attr.value).into_owned();
                                if let Some((author, date, ctype)) = tracked_changes_map.get(&cid) {
                                    output.push_str(&format!(
                                        "<{} data-author=\"{}\" data-date=\"{}\">",
                                        ctype,
                                        marksmen_xml_read::escape(author),
                                        marksmen_xml_read::escape(date)
                                    ));
                                }
                            }
                        }
                    }
                    b"text:change-end" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"text:change-id" {
                                let cid = String::from_utf8_lossy(&attr.value).into_owned();
                                if let Some((_, _, ctype)) = tracked_changes_map.get(&cid) {
                                    output.push_str(&format!("</{}>", ctype));
                                }
                            }
                        }
                    }
                    b"draw:image" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"xlink:href" {
                                let href = String::from_utf8_lossy(&attr.value).into_owned();
                                let mut emitted_path = href.clone();
                                if let Some(out_dir) = media_out_dir {
                                    if let Ok(mut img_file) = archive.by_name(&href) {
                                        let file_name = std::path::Path::new(&href)
                                            .file_name()
                                            .unwrap_or_default();
                                        let dest_path = out_dir.join(file_name);
                                        let mut bytes = Vec::new();
                                        if img_file.read_to_end(&mut bytes).is_ok() {
                                            let _ = std::fs::write(&dest_path, bytes);
                                        }
                                        if let Some(out_dir_name) = out_dir.file_name() {
                                            emitted_path = format!(
                                                "{}/{}",
                                                out_dir_name.to_string_lossy(),
                                                file_name.to_string_lossy()
                                            );
                                        }
                                    }
                                } else {
                                    if let Ok(mut img_file) = archive.by_name(&href) {
                                        let mut bytes = Vec::new();
                                        if img_file.read_to_end(&mut bytes).is_ok() {
                                            use base64::Engine;
                                            let b64 = base64::engine::general_purpose::STANDARD
                                                .encode(&bytes);
                                            let ext = std::path::Path::new(&href)
                                                .extension()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .to_lowercase();
                                            let mime = match ext.as_str() {
                                                "png" => "image/png",
                                                "jpg" | "jpeg" => "image/jpeg",
                                                "gif" => "image/gif",
                                                "svg" => "image/svg+xml",
                                                _ => "image/png",
                                            };
                                            emitted_path = format!("data:{};base64,{}", mime, b64);
                                        }
                                    }
                                }
                                output.push_str(&format!(
                                    "![image]({})",
                                    marksmen_xml_read::escape(&emitted_path)
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"text:p" | b"text:h" | b"text:hidden-paragraph" => {
                    if in_display_math_para {
                        let math_text = display_math_text.trim();
                        if !math_text.is_empty() {
                            if !output.ends_with("\n\n") && !output.is_empty() {
                                output.push_str("\n\n");
                            }
                            output.push_str("$$\n");
                            output.push_str(math_text);
                            output.push_str("\n$$");
                        }
                        in_display_math_para = false;
                        display_math_text.clear();
                    } else if in_hidden_meta {
                        let meta = hidden_meta_text.trim();
                        if !meta.is_empty() {
                            if !output.ends_with("\n\n") && !output.is_empty() {
                                output.push_str("\n\n");
                            }
                            output.push_str("$$");
                            output.push_str(meta);
                            output.push_str("$$");
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
                    output.push('\n');
                    tr_count += 1;
                    if tr_count == 1 {
                        output.push('|');
                        for i in 0..tc_count {
                            let align = tc_alignments.get(i).copied().unwrap_or(0);
                            match align {
                                1 => output.push_str(" :---: |"),
                                2 => output.push_str(" ---: |"),
                                _ => output.push_str(" :--- |"),
                            }
                        }
                        output.push('\n');
                    }
                }
                b"table:table" => {
                    in_tbl -= 1;
                    output.push('\n');
                }
                b"text:list" => {
                    list_ordered_stack.pop();
                    list_counter_stack.pop();
                }
                b"text:span" => {
                    in_span = false;
                    is_bold = false;
                    is_italic = false;
                    is_strikethrough = false;
                    is_inline_math = false;
                    is_code = false;
                    is_underline = false;
                    is_sub = false;
                    is_sup = false;
                    if in_hidden_span_meta {
                        let meta = hidden_span_meta_text.trim();
                        if !meta.is_empty() {
                            output.push('$');
                            output.push_str(meta);
                            output.push('$');
                        }
                        in_hidden_span_meta = false;
                        hidden_span_meta_text.clear();
                    }
                }
                b"text:a" => {
                    output.push_str(&format!("]({})", link_href));
                    link_href.clear();
                }
                b"text:changed-region" => {
                    tracked_changes_map.insert(
                        current_change_id.clone(),
                        (
                            current_creator.clone(),
                            current_date.clone(),
                            current_change_type.clone(),
                        ),
                    );
                }
                b"dc:creator" => {
                    in_creator = false;
                }
                b"dc:date" => {
                    in_date = false;
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default().into_owned();
                if in_creator {
                    current_creator.push_str(&text);
                }
                if in_date {
                    current_date.push_str(&text);
                }
                if in_display_math_para {
                    display_math_text.push_str(&text);
                    continue;
                }
                if text.trim().is_empty()
                    && !is_code
                    && !is_inline_math
                    && !is_display_math
                    && !in_hidden_meta
                    && !in_hidden_span_meta
                {
                    continue;
                }
                if in_hidden_meta {
                    hidden_meta_text.push_str(&text);
                    continue;
                }
                if in_hidden_span_meta {
                    hidden_span_meta_text.push_str(&text);
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
                            if is_code {
                                core_text = format!("`{}`", core_text);
                            }
                            if is_bold {
                                core_text = format!("**{}**", core_text);
                            }
                            if is_strikethrough {
                                core_text = format!("~~{}~~", core_text);
                            }
                            if is_italic && !core_text.contains("$") {
                                // Only wrap if it's not pre-wrapped math
                                core_text = format!("*{}*", core_text);
                            }
                            if is_underline {
                                core_text = format!("<u>{}</u>", core_text);
                            }
                            if is_sub {
                                core_text = format!("<sub>{}</sub>", core_text);
                            }
                            if is_sup {
                                core_text = format!("<sup>{}</sup>", core_text);
                            }
                        }
                        formatted = format!(
                            "{}{}{}",
                            " ".repeat(lead_chars),
                            core_text,
                            " ".repeat(trail_chars)
                        );
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

        let parsed = parse_odt(&synthetic_odt(content_xml), None).unwrap();
        assert!(parsed.contains("## Section Title"));
        assert!(parsed.contains("$$\nx + y\n$$"));
        assert!(parsed.contains("![Architecture Diagram](./architecture.svg)"));
        assert!(parsed.contains("$a^2+b^2$"));
    }
}
