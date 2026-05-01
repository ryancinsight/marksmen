//! Reader for Microsoft Word OpenXML (`.docx`) archives.
//!
//! Reconstructs a structured Markdown parsing stream by executing a rigorous structural traversal
//! over `word/document.xml` nodes and interpreting `w:p`, `w:r`, and OMML mathematical runs.

use anyhow::{Context, Result};
use marksmen_xml_read::Event;
use marksmen_xml_read::Reader;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

/// Analytically extracts `.docx` binary payloads into a mathematically equivalent Markdown string.
/// Traverses `<w:p>`, `<w:r>`, and `<w:t>` elements, evaluating nested styles for bold, italic,
/// and restoring mathematical `$inline$` or `$$display$$` syntax from `Cambria Math` tags.
///
/// If `media_out_dir` is provided, traverses `w:drawing` logic, performs `a:blip` mapping through
/// `word/_rels/document.xml.rels`, and isolates binary sub streams to local IO limits.

pub fn parse_docx(bytes: &[u8], media_out_dir: Option<&Path>) -> Result<String> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).context("Failed to parse bytes as a ZIP DOCX archive")?;

    let mut rels_map: HashMap<String, String> = HashMap::new();
    if let Ok(mut rels_file) = archive.by_name("word/_rels/document.xml.rels") {
        let mut rels_xml = String::new();
        if rels_file.read_to_string(&mut rels_xml).is_ok() {
            let mut reader = Reader::from_str(&rels_xml);
            reader.config_mut().trim_text(true);
            loop {
                match reader.read_event() {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                        if e.name().as_ref() == b"Relationship" {
                            let mut id = String::new();
                            let mut target = String::new();
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"Id" {
                                    id = String::from_utf8_lossy(&attr.value).into_owned();
                                } else if attr.key.as_ref() == b"Target" {
                                    target = String::from_utf8_lossy(&attr.value).into_owned();
                                }
                            }
                            if !id.is_empty() && !target.is_empty() {
                                rels_map.insert(id, target);
                            }
                        }
                    }
                    Ok(Event::Eof) | Err(_) => break,
                    _ => {}
                }
            }
        }
    }

    let mut comments_map: HashMap<String, (String, String)> = HashMap::new();
    if let Ok(mut comments_file) = archive.by_name("word/comments.xml") {
        let mut comments_xml = String::new();
        if comments_file.read_to_string(&mut comments_xml).is_ok() {
            let mut reader = Reader::from_str(&comments_xml);
            reader.config_mut().trim_text(true);
            let mut current_id = String::new();
            let mut current_author = String::new();
            let mut in_comment = false;
            let mut comment_text = String::new();
            loop {
                match reader.read_event() {
                    Ok(Event::Start(e)) => {
                        let name = e.name();
                        if name.as_ref() == b"w:comment" {
                            in_comment = true;
                            comment_text.clear();
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"w:id" {
                                    current_id = String::from_utf8_lossy(&attr.value).into_owned();
                                } else if attr.key.as_ref() == b"w:author" {
                                    current_author =
                                        String::from_utf8_lossy(&attr.value).into_owned();
                                }
                            }
                        }
                    }
                    Ok(Event::Text(e)) => {
                        if in_comment {
                            comment_text.push_str(&e.unescape().unwrap_or_default());
                        }
                    }
                    Ok(Event::End(e)) => {
                        if e.name().as_ref() == b"w:comment" {
                            in_comment = false;
                            if !current_id.is_empty() {
                                comments_map.insert(
                                    current_id.clone(),
                                    (current_author.clone(), comment_text.clone()),
                                );
                            }
                        }
                    }
                    Ok(Event::Eof) | Err(_) => break,
                    _ => {}
                }
            }
        }
    }

    let mut header_text = String::new();
    let mut header_xml = String::new();
    if let Ok(mut header_file) = archive.by_name("word/header1.xml") {
        let _ = header_file.read_to_string(&mut header_xml);
    }
    if !header_xml.is_empty() {
        if let Ok(parsed) = parse_xml_payload(
            &mut archive,
            &header_xml,
            &comments_map,
            &rels_map,
            media_out_dir,
        ) {
            header_text = parsed;
        }
    }

    let mut footnotes_xml = String::new();
    if let Ok(mut footnotes_file) = archive.by_name("word/footnotes.xml") {
        let _ = footnotes_file.read_to_string(&mut footnotes_xml);
    }
    let mut footnotes_map: HashMap<String, String> = HashMap::new();
    if !footnotes_xml.is_empty() {
        let mut iter = footnotes_xml.split("<w:footnote ");
        iter.next();
        for block in iter {
            if let Some(id_start) = block.find("w:id=\"") {
                let id_rest = &block[id_start + 6..];
                if let Some(id_end) = id_rest.find('"') {
                    let id = &id_rest[..id_end];
                    if id != "-1" && id != "0" {
                        if let Some(end_idx) = block.find("</w:footnote>") {
                            let footnote_xml =
                                format!("<w:footnote>{}</w:footnote>", &block[..end_idx]);
                            if let Ok(parsed) = parse_xml_payload(
                                &mut archive,
                                &footnote_xml,
                                &comments_map,
                                &rels_map,
                                media_out_dir,
                            ) {
                                footnotes_map.insert(id.to_string(), parsed.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    let mut doc_xml = String::new();
    {
        let mut file = archive
            .by_name("word/document.xml")
            .context("Missing word/document.xml in DOCX archive")?;
        file.read_to_string(&mut doc_xml)
            .context("Failed to read word/document.xml")?;
    }
    let doc_text = parse_xml_payload(
        &mut archive,
        &doc_xml,
        &comments_map,
        &rels_map,
        media_out_dir,
    )?;

    let mut footer_text = String::new();
    let mut footer_xml = String::new();
    if let Ok(mut footer_file) = archive.by_name("word/footer1.xml") {
        let _ = footer_file.read_to_string(&mut footer_xml);
    }
    if !footer_xml.is_empty() {
        if let Ok(parsed) = parse_xml_payload(
            &mut archive,
            &footer_xml,
            &comments_map,
            &rels_map,
            media_out_dir,
        ) {
            footer_text = parsed;
        }
    }

    // Extract the page metadata comment from doc_text and hoist to TOP of final_out
    // so the DOCX writer sees it as the first event via peek().
    let (page_meta_line, doc_text_body) = if doc_text.starts_with("<!-- page:") {
        if let Some(end) = doc_text.find("-->") {
            let meta = doc_text[..end + 3].trim().to_string();
            let rest = doc_text[end + 3..].trim_start_matches('\n').to_string();
            (Some(meta), rest)
        } else {
            (None, doc_text.clone())
        }
    } else {
        (None, doc_text.clone())
    };

    let mut final_out = String::new();
    if let Some(meta) = page_meta_line {
        final_out.push_str(&format!("{}\n\n", meta));
    }
    if !header_text.is_empty() {
        final_out.push_str(&format!(
            "<header>\n\n{}\n\n</header>\n\n",
            header_text.trim()
        ));
    }
    final_out.push_str(&doc_text_body);
    if !footer_text.is_empty() {
        final_out.push_str(&format!(
            "\n\n<footer>\n\n{}\n\n</footer>",
            footer_text.trim()
        ));
    }

    let mut fn_keys: Vec<_> = footnotes_map.keys().collect();
    fn_keys.sort_by_key(|k| k.parse::<i32>().unwrap_or(0));
    for k in fn_keys {
        let val = &footnotes_map[k];
        if !val.is_empty() {
            final_out.push_str(&format!("\n\n[^{}]: {}", k, val));
        }
    }

    Ok(final_out.trim().to_string())
}

fn parse_xml_payload(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    xml_content: &str,
    comments_map: &HashMap<String, (String, String)>,
    rels_map: &HashMap<String, String>,
    media_out_dir: Option<&Path>,
) -> Result<String> {
    let mut reader = Reader::from_str(xml_content);
    reader.config_mut().trim_text(false);

    let mut output = String::new();
    let mut in_p = false;
    let mut in_r = false;
    let mut in_t = false;

    // Formatting context for the current run
    let mut is_bold = false;
    let mut is_italic = false;
    let mut is_math = false;
    let mut is_code = false;
    let mut p_heading_level = 0; // 0 means not a heading
    let mut p_aligned_center = false;
    let mut p_span_center_emitted = false;
    let mut is_underline = false;
    let mut is_strike = false;
    let mut is_subscript = false;
    let mut is_superscript = false;
    let mut in_quote = false;
    let mut in_code_block = false;
    let mut is_highlight = false;
    let mut p_custom_style: Option<String> = None; // Non-structural Word style for {.StyleName} emission
    let mut in_hyperlink_r_id: Option<String> = None;
    let mut hyperlink_start_idx: Option<usize> = None;

    // Field code (w:fldChar / w:instrText) tracking
    let mut in_fld = false; // true between fldChar begin..end
    let mut in_fld_instr = false; // true while reading w:instrText
    let mut fld_instr_buf = String::new(); // accumulates instrText content
    let mut in_fld_cached = false; // true inside fldChar separate..end (skip display text)
    let mut fld_eval_buf = String::new(); // accumulates evaluated result

    // Node stack to track nested blocks (like Table vs Paragraph)
    let mut in_tbl = 0;
    // Per-table-level row counter. Pushed when entering a table, popped on exit.
    // tr_count_stack.last() == current table's row count; separator is injected after row 1.
    let mut tr_count_stack: Vec<u32> = Vec::new();
    let mut tc_count = 0;
    let mut tc_alignments: Vec<u8> = Vec::new(); // 1 = center, 2 = right
    let mut tc_state_stack: Vec<(u32, u8, Option<String>)> = Vec::new(); // (grid_span, alignment, bg_color)
    // When in_tbl > 1 we buffer the entire nested table HTML here and emit it
    // atomically at </w:tbl> so pulldown-cmark receives one contiguous HTML block
    // rather than fragmented per-tag events, enabling the writer's HTML parser.
    let mut nested_html_buf: Option<String> = None;

    let mut drawing_name = String::new();
    let mut drawing_descr = String::new();
    let mut drawing_target_file = String::new();

    // List state: detect w:numId and w:ilvl from w:numPr within each w:p.
    // numId=1 => bullet, numId=2 => decimal. Track counters per ilvl.
    let mut p_num_id: u32 = 0; // 0 = not a list paragraph
    let mut p_ilvl: usize = 0; // indent level (0-indexed)
    let mut p_list_marker_emitted = false;
    // Counters per ilvl, indexed by ilvl value (grows on demand).
    let mut list_counters: Vec<u32> = Vec::new();
    // Track the previous ilvl to reset counters when leaving deeper levels.
    let mut prev_ilvl: usize = 0;

    // Page geometry captured from sectPr — emitted as first-line comment in output
    // so the DOCX writer can reconstruct the correct page size/margins.
    let mut pg_w: u32 = 0;
    let mut pg_h: u32 = 0;
    let mut pg_margin_t: i32 = -1;
    let mut pg_margin_r: i32 = -1;
    let mut pg_margin_b: i32 = -1;
    let mut pg_margin_l: i32 = -1;

    loop {
        let event = reader.read_event();
        match &event {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let is_empty = matches!(event, Ok(Event::Empty(_)));
                let name = e.name();
                match name.as_ref() {
                    b"w:pgSz" => {
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"w:w" => {
                                    pg_w =
                                        String::from_utf8_lossy(&attr.value).parse().unwrap_or(0);
                                }
                                b"w:h" => {
                                    pg_h =
                                        String::from_utf8_lossy(&attr.value).parse().unwrap_or(0);
                                }
                                _ => {}
                            }
                        }
                    }
                    b"w:pgMar" => {
                        for attr in e.attributes().flatten() {
                            let v: i32 = String::from_utf8_lossy(&attr.value).parse().unwrap_or(-1);
                            match attr.key.as_ref() {
                                b"w:top" => {
                                    pg_margin_t = v;
                                }
                                b"w:right" => {
                                    pg_margin_r = v;
                                }
                                b"w:bottom" => {
                                    pg_margin_b = v;
                                }
                                b"w:left" => {
                                    pg_margin_l = v;
                                }
                                _ => {}
                            }
                        }
                    }
                    b"w:fldSimple" => {
                        // Handle inline field like NUMPAGES
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:instr" {
                                let instr =
                                    String::from_utf8_lossy(&attr.value).trim().to_uppercase();
                                if instr.starts_with("NUMPAGES")
                                    || instr.starts_with("SECTIONPAGES")
                                {
                                    output.push_str("<!-- TOTAL_PAGES -->");
                                } else if instr.starts_with("PAGE") {
                                    output.push_str("<!-- PAGE_NUM -->");
                                }
                                // Mark that subsequent Text event inside fldSimple is stale cache
                                in_fld_cached = true;
                            }
                        }
                    }
                    b"w:commentRangeStart" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:id" {
                                let id = String::from_utf8_lossy(&attr.value);
                                if let Some((author, content)) = comments_map.get(id.as_ref()) {
                                    let tag_str = format!(
                                        "<mark class=\"comment\" data-author=\"{}\" data-content=\"{}\">",
                                        marksmen_xml_read::escape(author),
                                        marksmen_xml_read::escape(content)
                                    );
                                    if in_tbl > 1 {
                                        if let Some(buf) = nested_html_buf.as_mut() {
                                            buf.push_str(&tag_str);
                                        }
                                    } else {
                                        output.push_str(&tag_str);
                                    }
                                }
                            }
                        }
                    }
                    b"w:commentRangeEnd" => {
                        if in_tbl > 1 {
                            if let Some(buf) = nested_html_buf.as_mut() {
                                buf.push_str("</mark>");
                            }
                        } else {
                            output.push_str("</mark>");
                        }
                    }
                    b"w:ins" | b"w:del" => {
                        let mut author = String::new();
                        let mut date = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:author" {
                                author = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                            if attr.key.as_ref() == b"w:date" {
                                date = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                        }
                        let tag = if name.as_ref() == b"w:ins" {
                            "ins"
                        } else {
                            "del"
                        };
                        if !is_empty {
                            let tag_str = if author.is_empty() && date.is_empty() {
                                format!("<{}>", tag)
                            } else {
                                format!(
                                    "<{} data-author=\"{}\" data-date=\"{}\">",
                                    tag,
                                    marksmen_xml_read::escape(&author),
                                    marksmen_xml_read::escape(&date)
                                )
                            };
                            if in_tbl > 1 {
                                if let Some(buf) = nested_html_buf.as_mut() {
                                    buf.push_str(&tag_str);
                                }
                            } else {
                                output.push_str(&tag_str);
                            }
                        }
                    }
                    b"w:p" => {
                        in_p = true;
                        p_heading_level = 0;
                        p_aligned_center = false;
                        p_span_center_emitted = false;
                        in_quote = false;
                        p_num_id = 0;
                        p_ilvl = 0;
                        p_list_marker_emitted = false;
                        is_highlight = false;
                        if in_tbl == 0 {
                            if output.len() > 0 {
                                if !output.ends_with("\n\n") {
                                    if output.ends_with('\n') {
                                        output.push('\n');
                                    } else {
                                        output.push_str("\n\n");
                                    }
                                }
                            }
                        }
                    }
                    b"w:tbl" => {
                        in_tbl += 1;
                        tr_count_stack.push(0); // push a fresh row counter for this table
                        if in_tbl > 1 {
                            if nested_html_buf.is_none() {
                                nested_html_buf = Some(String::new());
                            }
                            nested_html_buf
                                .as_mut()
                                .unwrap()
                                .push_str("<table class=\"nested\">");
                        } else if output.len() > 0 && !output.ends_with("\n\n") {
                            output.push_str("\n\n");
                        }
                    }
                    b"w:tr" => {
                        let buf = if in_tbl > 1 {
                            nested_html_buf.as_mut().map(|b| b as &mut String)
                        } else {
                            None
                        };
                        if let Some(b) = buf {
                            b.push_str("<tr>");
                        } else {
                            output.push_str("| ");
                        }
                        tc_count = 0;
                        tc_alignments.clear();
                    }
                    b"w:tc" => {
                        if in_tbl > 1 {
                            nested_html_buf.as_mut().unwrap().push_str("<td>");
                        }
                        tc_count += 1;
                        tc_state_stack.push((1, 0, None));
                    }
                    b"w:shd" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:fill" {
                                let fill_val = String::from_utf8_lossy(&attr.value);
                                if fill_val != "auto" && fill_val != "clear" {
                                    if let Some(state) = tc_state_stack.last_mut() {
                                        state.2 = Some(fill_val.into_owned());
                                    }
                                }
                            }
                        }
                    }
                    b"w:gridSpan" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val" {
                                if let Some(state) = tc_state_stack.last_mut() {
                                    state.0 =
                                        String::from_utf8_lossy(&attr.value).parse().unwrap_or(1);
                                }
                            }
                        }
                    }
                    b"w:pStyle" => {
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"w:val" {
                                    let val = a.value;
                                    if val.as_ref() == b"Quote" {
                                        in_quote = true;
                                    } else if val.as_ref() == b"CodeBlock" {
                                        if !output.ends_with("\n\n") && !output.ends_with("\n") {
                                            output.push_str("\n\n");
                                        }
                                        output.push_str("```\n");
                                        in_code_block = true;
                                    } else if val.starts_with(b"Heading") && val.len() == 8 {
                                        let level = val[7] - b'0';
                                        if level >= 1 && level <= 6 {
                                            p_heading_level = level;
                                        }
                                    } else {
                                        let style_name = String::from_utf8_lossy(&val).into_owned();
                                        let is_internal = style_name == "Normal"
                                            || style_name == "DefaultParagraphFont"
                                            || style_name == "Header"
                                            || style_name == "Footer"
                                            || style_name.starts_with("a-")
                                            || style_name.is_empty();
                                        if !is_internal {
                                            p_custom_style = Some(style_name);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    b"w:numId" => {
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"w:val" {
                                    p_num_id =
                                        String::from_utf8_lossy(&a.value).parse().unwrap_or(0);
                                }
                            }
                        }
                    }
                    b"w:ilvl" => {
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"w:val" {
                                    p_ilvl = String::from_utf8_lossy(&a.value).parse().unwrap_or(0);
                                }
                            }
                        }
                    }
                    b"w:jc" => {
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"w:val" {
                                    if a.value.as_ref() == b"center" {
                                        p_aligned_center = true;
                                        if let Some(state) = tc_state_stack.last_mut() {
                                            state.1 = 1;
                                        }
                                    } else if a.value.as_ref() == b"right" {
                                        if let Some(state) = tc_state_stack.last_mut() {
                                            state.1 = 2;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    b"w:r" => {
                        in_r = true;
                        is_bold = false;
                        is_italic = false;
                        is_math = false;
                        is_code = false;
                        is_highlight = false;

                        if p_num_id > 0 && !p_list_marker_emitted && in_p {
                            p_list_marker_emitted = true;
                            let indent = "    ".repeat(p_ilvl);
                            if p_num_id == 2 {
                                if p_ilvl > prev_ilvl {
                                    while list_counters.len() <= p_ilvl {
                                        list_counters.push(0);
                                    }
                                    list_counters[p_ilvl] = 0;
                                }
                                while list_counters.len() <= p_ilvl {
                                    list_counters.push(0);
                                }
                                list_counters[p_ilvl] += 1;
                                output.push_str(&format!("{}{}. ", indent, list_counters[p_ilvl]));
                            } else {
                                output.push_str(&format!("{}- ", indent));
                            }
                            prev_ilvl = p_ilvl;
                        }

                        if p_heading_level > 0 && in_p {
                            let marks = "#".repeat(p_heading_level as usize);
                            output.push_str(&format!("{} ", marks));
                            p_heading_level = 0;
                        } else if in_quote && in_p {
                            output.push_str("> ");
                            in_quote = false;
                        }

                        if p_aligned_center && in_p && !p_span_center_emitted {
                            let tag_str = "<mark class=\"align-center\">";
                            if in_tbl > 1 {
                                if let Some(buf) = nested_html_buf.as_mut() {
                                    buf.push_str(tag_str);
                                }
                            } else {
                                output.push_str(tag_str);
                            }
                            p_span_center_emitted = true;
                        }
                    }
                    b"w:fldChar" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:fldCharType" {
                                let fld_type = attr.value.as_ref();
                                if fld_type == b"begin" {
                                    in_fld = true;
                                    in_fld_cached = false;
                                    fld_instr_buf.clear();
                                    fld_eval_buf.clear();
                                } else if fld_type == b"separate" {
                                    in_fld_cached = true;
                                    in_fld_instr = false;
                                } else if fld_type == b"end" {
                                    if in_fld {
                                        if !fld_instr_buf.is_empty() || !fld_eval_buf.is_empty() {
                                            output.push_str(&format!(
                                                "<span data-field=\"{}\">{}</span>",
                                                marksmen_xml_read::escape(fld_instr_buf.trim()),
                                                marksmen_xml_read::escape(fld_eval_buf.trim())
                                            ));
                                        }
                                        in_fld = false;
                                        in_fld_cached = false;
                                        in_fld_instr = false;
                                    }
                                }
                            }
                        }
                    }
                    b"w:instrText" => {
                        in_fld_instr = true;
                    }
                    b"w:t" | b"w:delText" => in_t = true,
                    b"w:b" => is_bold = true,
                    b"w:i" => is_italic = true,
                    b"w:u" => is_underline = true,
                    b"w:strike" | b"w:dstrike" => {
                        let mut strike_val = true;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val" {
                                if attr.value.as_ref() == b"false" || attr.value.as_ref() == b"0" {
                                    strike_val = false;
                                }
                            }
                        }
                        is_strike = strike_val;
                    }
                    b"w:highlight" => {
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"w:val" && a.value.as_ref() != b"none" {
                                    is_highlight = true;
                                }
                            }
                        }
                    }
                    b"w:vertAlign" => {
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"w:val" {
                                    if a.value.as_ref() == b"subscript" {
                                        is_subscript = true;
                                    } else if a.value.as_ref() == b"superscript" {
                                        is_superscript = true;
                                    }
                                }
                            }
                        }
                    }
                    b"w:rFonts" => {
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"w:ascii" {
                                    if a.value.as_ref() == b"Cambria Math" {
                                        is_math = true;
                                    } else if a.value.as_ref() == b"Consolas" {
                                        is_code = true;
                                    }
                                }
                            }
                        }
                    }
                    b"w:hyperlink" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"r:id" {
                                in_hyperlink_r_id =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                                hyperlink_start_idx = Some(output.len());
                            }
                        }
                    }
                    b"w:footnoteReference" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:id" {
                                let id = String::from_utf8_lossy(&attr.value);
                                output.push_str(&format!("[^{}]", id));
                            }
                        }
                    }
                    b"w:br" => {
                        let mut is_page = false;
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"w:type" && a.value.as_ref() == b"page" {
                                    is_page = true;
                                }
                            }
                        }
                        if is_page {
                            output.push_str("\n<!-- pagebreak -->\n");
                        } else if in_quote {
                            output.push_str("\n> ");
                        } else if in_tbl > 1 {
                            if let Some(buf) = nested_html_buf.as_mut() {
                                buf.push_str("<br/>");
                            }
                        } else if in_tbl > 0 {
                            output.push_str("<br/>");
                        } else {
                            output.push_str("\n");
                        }
                    }
                    b"w:drawing" => {
                        drawing_name.clear();
                        drawing_descr.clear();
                        drawing_target_file.clear();
                    }
                    b"a:blip" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"r:embed" {
                                let rid = String::from_utf8_lossy(&attr.value).to_string();
                                if let Some(target) = rels_map.get(&rid) {
                                    if let Some(out_dir) = media_out_dir {
                                        if let Some(file_name) = Path::new(target).file_name() {
                                            let dest_path = out_dir.join(file_name);
                                            let clean_target = target.replace("\\", "/");
                                            let stripped = clean_target.trim_start_matches('/');
                                            let candidates = [
                                                stripped.to_string(),
                                                format!("word/{}", stripped),
                                            ];
                                            for cand in &candidates {
                                                if let Ok(mut img_file) = archive.by_name(cand) {
                                                    if let Ok(mut out_file) =
                                                        std::fs::File::create(&dest_path)
                                                    {
                                                        let _ = std::io::copy(
                                                            &mut img_file,
                                                            &mut out_file,
                                                        );
                                                    }
                                                    break;
                                                }
                                            }
                                            if let Some(out_dir_name) = out_dir.file_name() {
                                                let relative_path = Path::new(out_dir_name)
                                                    .join(file_name)
                                                    .to_string_lossy()
                                                    .replace("\\", "/");
                                                drawing_target_file = relative_path;
                                            }
                                        }
                                    } else {
                                        // Embed as Base64 data URI if no output directory is provided
                                        let clean_target = target.replace("\\", "/");
                                        let stripped = clean_target.trim_start_matches('/');
                                        let candidates =
                                            [stripped.to_string(), format!("word/{}", stripped)];
                                        for cand in &candidates {
                                            if let Ok(mut img_file) = archive.by_name(cand) {
                                                use std::io::Read;
                                                let mut buf = Vec::new();
                                                if img_file.read_to_end(&mut buf).is_ok() {
                                                    use base64::Engine;
                                                    let b64 =
                                                        base64::engine::general_purpose::STANDARD
                                                            .encode(&buf);
                                                    let ext = Path::new(cand)
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
                                                    drawing_target_file =
                                                        format!("data:{};base64,{}", mime, b64);
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    b"wp:docPr" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"name" {
                                drawing_name = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                            if attr.key.as_ref() == b"descr" {
                                drawing_descr = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:ins" => {
                    if in_tbl > 1 {
                        if let Some(buf) = nested_html_buf.as_mut() {
                            buf.push_str("</ins>");
                        }
                    } else {
                        output.push_str("</ins>");
                    }
                }
                b"w:del" => {
                    if in_tbl > 1 {
                        if let Some(buf) = nested_html_buf.as_mut() {
                            buf.push_str("</del>");
                        }
                    } else {
                        output.push_str("</del>");
                    }
                }
                b"w:p" => {
                    in_p = false;
                    if p_span_center_emitted {
                        if in_tbl > 1 {
                            if let Some(buf) = nested_html_buf.as_mut() {
                                buf.push_str("</mark>");
                            }
                        } else {
                            output.push_str("</mark>");
                        }
                    }
                    if in_code_block {
                        if !output.ends_with('\n') {
                            output.push_str("\n");
                        }
                        output.push_str("```\n");
                        in_code_block = false;
                    }
                    if in_tbl == 0 {
                        if let Some(style) = p_custom_style.take() {
                            if !output.ends_with("\n\n") {
                                if output.ends_with('\n') {
                                    output.push('\n');
                                } else {
                                    output.push_str("\n\n");
                                }
                            }
                            output.push_str(&format!("{{.{}}}\n\n", style));
                        }
                    } else {
                        p_custom_style = None;
                    }
                    if in_tbl > 0 && !output.ends_with("| ") {
                        if in_tbl > 1 {
                            if let Some(buf) = nested_html_buf.as_mut() {
                                buf.push_str("<br/>");
                            }
                        } else {
                            if !output.ends_with("<!-- P_BR -->") {
                                output.push_str("<!-- P_BR -->");
                            }
                        }
                    }
                }
                b"w:hyperlink" => {
                    if let (Some(r_id), Some(start_idx)) =
                        (in_hyperlink_r_id.take(), hyperlink_start_idx.take())
                    {
                        let url = rels_map.get(&r_id).cloned().unwrap_or(r_id);
                        if start_idx < output.len() {
                            let text_content = output[start_idx..].to_string();
                            output.truncate(start_idx);
                            output.push_str(&format!("[{}]({} \"\")", text_content, url));
                        }
                    }
                }
                b"w:r" => {
                    in_r = false;
                    is_underline = false;
                    is_strike = false;
                    is_subscript = false;
                    is_superscript = false;
                }
                b"w:t" | b"w:delText" => in_t = false,
                b"w:tc" => {
                    let mut tc_span = 1;
                    let mut tc_align = 0;
                    let mut tc_bg = None;
                    if let Some(state) = tc_state_stack.pop() {
                        tc_span = state.0;
                        tc_align = state.1;
                        tc_bg = state.2;
                    }

                    if in_tbl > 1 {
                        if let Some(buf) = nested_html_buf.as_mut() {
                            while buf.ends_with("<br/>") {
                                let l = buf.len() - 5;
                                buf.truncate(l);
                            }
                            if let Some(bg) = tc_bg {
                                buf.push_str(&format!("<!-- BG_COLOR:{} -->", bg));
                            }
                            if tc_span > 1 {
                                buf.push_str(&format!("<!-- COLSPAN:{} -->", tc_span));
                            }
                            buf.push_str("</td>");
                        }
                    } else {
                        tc_alignments.push(tc_align);
                        if let Some(bg) = tc_bg {
                            output.push_str(&format!(" <!-- BG_COLOR:{} --> ", bg));
                        }
                        output.push_str(" | ");
                        for _ in 1..tc_span {
                            tc_alignments.push(tc_align);
                            output.push_str(" <!-- COLSPAN --> | ");
                        }
                    }
                }
                b"w:tr" => {
                    if in_tbl > 1 {
                        if let Some(buf) = nested_html_buf.as_mut() {
                            buf.push_str("</tr>");
                        }
                    } else {
                        output.push_str("\n");
                        if let Some(cnt) = tr_count_stack.last_mut() {
                            *cnt += 1;
                            if *cnt == 1 {
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
                    }
                }
                b"w:tbl" => {
                    in_tbl -= 1;
                    let _ = tr_count_stack.pop();
                    if in_tbl >= 1 {
                        if let Some(buf) = nested_html_buf.as_mut() {
                            buf.push_str("</table>");
                        }
                        if in_tbl == 1 {
                            if let Some(full_html) = nested_html_buf.take() {
                                output.push_str(&full_html);
                            }
                        }
                    } else {
                        output.push_str("\n");
                    }
                }
                b"w:drawing" => {
                    let alt = if !drawing_name.is_empty() {
                        &drawing_name
                    } else {
                        "Image"
                    };
                    let path = if !drawing_target_file.is_empty() {
                        &drawing_target_file
                    } else {
                        &drawing_descr
                    };
                    let valid_path = path.replace(" ", "%20");
                    if in_tbl > 1 {
                        if let Some(buf) = nested_html_buf.as_mut() {
                            buf.push_str(&format!(
                                "<img src=\"{}\" alt=\"{}\" />",
                                valid_path, alt
                            ));
                        }
                    } else if in_tbl == 1 {
                        output.push_str(&format!("![{}]({})", alt, valid_path));
                    } else {
                        if output.len() > 0 && !output.ends_with("\n\n") {
                            output.push_str("\n\n");
                        }
                        output.push_str(&format!("![{}]({})\n\n", alt, valid_path));
                    }
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                let raw_text = e.unescape().unwrap_or_default().into_owned();
                if in_fld_instr {
                    fld_instr_buf.push_str(&raw_text);
                    continue;
                }
                if in_t {
                    if in_fld_cached {
                        fld_eval_buf.push_str(&raw_text);
                        continue;
                    }
                }
                if in_t && in_r && in_p {
                    let mut text = raw_text;
                    if !text.is_empty() {
                        // Detect synthetic bullet points injected by marksmen-docx mapping
                        if text.starts_with("•     ") {
                            let pad = if !output.ends_with("\n") && !output.ends_with("\n\n") {
                                "\n- "
                            } else {
                                "- "
                            };
                            text = text.replacen("•     ", pad, 1);
                        }

                        let mut formatted = text.clone();
                        if is_math {
                            if p_aligned_center {
                                formatted = format!("$$\n{}\n$$", formatted.trim());
                            } else {
                                formatted = format!("${}$", formatted.trim());
                            }
                            // Math always goes to main output even in nested cells
                            output.push_str(&formatted);
                        } else {
                            let lead_chars =
                                formatted.len() - formatted.trim_start_matches(' ').len();
                            let trail_chars =
                                formatted.len() - formatted.trim_end_matches(' ').len();
                            let mut core_text = formatted.trim().to_string();
                            if !core_text.is_empty() {
                                if is_code && !in_code_block {
                                    core_text = format!("`{}`", core_text);
                                }
                                if is_bold {
                                    core_text = format!("**{}**", core_text);
                                }
                                if is_italic {
                                    core_text = format!("*{}*", core_text);
                                }
                                if is_underline {
                                    core_text = format!("<u>{}</u>", core_text);
                                }
                                if is_strike {
                                    core_text = format!("~~{}~~", core_text);
                                }
                                if is_subscript {
                                    core_text = format!("<sub>{}</sub>", core_text);
                                }
                                if is_superscript {
                                    core_text = format!("<sup>{}</sup>", core_text);
                                }
                                if is_highlight {
                                    core_text =
                                        format!("<mark class=\"highlight\">{}</mark>", core_text);
                                }
                            }
                            formatted = format!(
                                "{}{}{}",
                                " ".repeat(lead_chars),
                                core_text,
                                " ".repeat(trail_chars)
                            );
                            // Route text to nested buffer when inside nested table cell.
                            // Use HTML tags instead of Markdown markers so pulldown-cmark
                            // does not re-parse ** or * as emphasis inside the HTML blob.
                            if in_tbl > 1 {
                                if let Some(buf) = nested_html_buf.as_mut() {
                                    // Reconstruct HTML from the original text, applying all formatting
                                    // as HTML tags (not Markdown markers) so pulldown-cmark does not
                                    // re-parse ** / * / _ as emphasis inside the raw HTML blob.
                                    let raw_trim = text.trim().to_string();
                                    let mut html_text =
                                        marksmen_xml_read::escape(&raw_trim).into_owned();
                                    if is_code {
                                        html_text = format!("<code>{}</code>", html_text);
                                    }
                                    if is_bold {
                                        html_text = format!("<strong>{}</strong>", html_text);
                                    }
                                    if is_italic {
                                        html_text = format!("<em>{}</em>", html_text);
                                    }
                                    if is_underline {
                                        html_text = format!("<u>{}</u>", html_text);
                                    }
                                    if is_strike {
                                        html_text = format!("~~{}~~", html_text);
                                    }
                                    if is_subscript {
                                        html_text = format!("<sub>{}</sub>", html_text);
                                    }
                                    if is_superscript {
                                        html_text = format!("<sup>{}</sup>", html_text);
                                    }
                                    if is_highlight {
                                        html_text = format!(
                                            "<mark class=\"highlight\">{}</mark>",
                                            html_text
                                        );
                                    }
                                    let html_fmt = format!(
                                        "{}{}{}",
                                        " ".repeat(lead_chars),
                                        html_text,
                                        " ".repeat(trail_chars)
                                    );
                                    buf.push_str(&html_fmt);
                                }
                            } else {
                                output.push_str(&formatted);
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break, // Gracefully truncate on corruption
            _ => {}
        }
    }

    let mut cleaned = output.trim().to_string();

    // Mathematically sew fragmented typography spans back together
    // caused by OOXML <w:r> boundary iterations
    let mut last = String::new();
    while last != cleaned {
        last = cleaned.clone();
        cleaned = cleaned.replace("* *", " ");
        cleaned = cleaned.replace("** **", " ");
        cleaned = cleaned.replace("_ _", " ");
        cleaned = cleaned.replace("~ ~", " ");
    }

    // Prepend page geometry as first-line comment when detected from sectPr.
    // Only emitted for document.xml parsing (pg_w > 0), not for header/footer payloads.
    if pg_w > 0 && pg_h > 0 {
        let margin_str = if pg_margin_t >= 0 {
            format!(
                " margin:{},{},{},{}",
                pg_margin_t, pg_margin_r, pg_margin_b, pg_margin_l
            )
        } else {
            String::new()
        };
        cleaned = format!(
            "<!-- page:{}x{}{} -->\n\n{}",
            pg_w, pg_h, margin_str, cleaned
        );
    }

    Ok(cleaned)
}
