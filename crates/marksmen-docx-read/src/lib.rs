//! Reader for Microsoft Word OpenXML (`.docx`) archives.
//!
//! Reconstructs a structured Markdown parsing stream by executing a rigorous structural traversal
//! over `word/document.xml` nodes and interpreting `w:p`, `w:r`, and OMML mathematical runs.

use anyhow::{Context, Result};
use std::io::{Cursor, Read};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::path::Path;
use std::collections::HashMap;

/// Analytically extracts `.docx` binary payloads into a mathematically equivalent Markdown string.
/// Traverses `<w:p>`, `<w:r>`, and `<w:t>` elements, evaluating nested styles for bold, italic,
/// and restoring mathematical `$inline$` or `$$display$$` syntax from `Cambria Math` tags.
///
/// If `media_out_dir` is provided, traverses `w:drawing` logic, performs `a:blip` mapping through
/// `word/_rels/document.xml.rels`, and isolates binary sub streams to local IO limits.

pub fn parse_docx(bytes: &[u8], media_out_dir: Option<&Path>) -> Result<String> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .context("Failed to parse bytes as a ZIP DOCX archive")?;

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

    let mut doc_xml = String::new();
    {
        let mut file = archive
            .by_name("word/document.xml")
            .context("Missing word/document.xml in DOCX archive")?;
        file.read_to_string(&mut doc_xml)
            .context("Failed to read word/document.xml")?;
    }

    let mut reader = Reader::from_str(&doc_xml);
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
    let mut is_underline = false;
    let mut is_subscript = false;
    let mut is_superscript = false;
    let mut in_quote = false;
    
    // Node stack to track nested blocks (like Table vs Paragraph)
    let mut in_tbl = 0;
    let mut tr_count = 0;
    let mut tc_count = 0;
    let mut tc_alignments: Vec<u8> = Vec::new(); // 1 = center, 2 = right
    let mut current_tc_alignment = 0;

    let mut drawing_name = String::new();
    let mut drawing_descr = String::new();
    let mut drawing_target_file = String::new();
    
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"w:p" => {
                        in_p = true;
                        p_heading_level = 0;
                        p_aligned_center = false;
                        in_quote = false;
                        if in_tbl == 0 {
                            if output.len() > 0 {
                                if !output.ends_with("\n") {
                                    output.push_str("\n\n");
                                } else if !output.ends_with("\n\n") {
                                    output.push_str("\n");
                                }
                            }
                        } else {
                            if !output.ends_with("| ") && !output.ends_with(" ") {
                                output.push_str(" ");
                            }
                        }
                    }
                    b"w:tbl" => {
                        in_tbl += 1;
                        tr_count = 0;
                        if output.len() > 0 && !output.ends_with("\n\n") {
                            output.push_str("\n\n");
                        }
                    }
                    b"w:tr" => {
                        output.push_str("| ");
                        tc_count = 0;
                        tc_alignments.clear();
                    }
                    b"w:tc" => {
                        tc_count += 1;
                        current_tc_alignment = 0;
                    }
                    b"w:pStyle" => {
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"w:val" {
                                    let val = a.value;
                                    if val.as_ref() == b"Quote" {
                                        in_quote = true;
                                    } else if val.starts_with(b"Heading") && val.len() == 8 {
                                        let level = val[7] - b'0';
                                        if level >= 1 && level <= 6 {
                                            p_heading_level = level;
                                        }
                                    }
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
                                        current_tc_alignment = 1;
                                    } else if a.value.as_ref() == b"right" {
                                        current_tc_alignment = 2;
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
                        
                        // If this paragraph is a heading and we haven't emitted the `#`s yet
                        // we inject them right before the first text run
                        if p_heading_level > 0 && in_p {
                            let marks = "#".repeat(p_heading_level as usize);
                            output.push_str(&format!("{} ", marks));
                            p_heading_level = 0; // Prevents re-emitting on subsequent runs
                        } else if in_quote && in_p {
                            output.push_str("> ");
                            in_quote = false; // Prevents re-emitting
                        }
                    }
                    b"w:t" => in_t = true,
                    b"w:b" => is_bold = true,
                    b"w:i" => is_italic = true,
                    b"w:u" => is_underline = true,
                    b"w:vertAlign" => {
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                if a.key.as_ref() == b"w:val" {
                                    if a.value.as_ref() == b"subscript" { is_subscript = true; }
                                    else if a.value.as_ref() == b"superscript" { is_superscript = true; }
                                }
                            }
                        }
                    }
                    b"w:rFonts" => {
                        // Check if ascii attr equals "Cambria Math" or "Consolas"
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
                                            let internal_path = format!("word/{}", target.replace("\\", "/"));
                                            if let Ok(mut img_file) = archive.by_name(&internal_path) {
                                                if let Ok(mut out_file) = std::fs::File::create(&dest_path) {
                                                    let _ = std::io::copy(&mut img_file, &mut out_file);
                                                }
                                            }
                                            if let Some(out_dir_name) = out_dir.file_name() {
                                                // Convert the path completely to forward slashes for cross-platform markdown.
                                                let relative_path = Path::new(out_dir_name).join(file_name).to_string_lossy().replace("\\", "/");
                                                drawing_target_file = relative_path;
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
                b"w:p" => {
                    in_p = false;
                }
                b"w:r" => {
                    in_r = false;
                    is_underline = false;
                    is_subscript = false;
                    is_superscript = false;
                }
                b"w:t" => in_t = false,
                b"w:tc" => {
                    tc_alignments.push(current_tc_alignment);
                    output.push_str(" | ");
                }
                b"w:tr" => {
                    output.push_str("\n");
                    tr_count += 1;
                    if tr_count == 1 {
                        // Inject markdown table header separator row after the first row
                        // using the number of columns encountered
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
                b"w:tbl" => {
                    in_tbl -= 1;
                    output.push_str("\n");
                }
                b"w:drawing" => {
                    if output.len() > 0 && !output.ends_with("\n\n") {
                        output.push_str("\n\n");
                    }
                    let alt = if !drawing_name.is_empty() { &drawing_name } else { "Image" };
                    let path = if !drawing_target_file.is_empty() { &drawing_target_file } else { &drawing_descr };
                    let valid_path = path.replace(" ", "%20");
                    output.push_str(&format!("![{}]({})\n\n", alt, valid_path));
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if in_t && in_r && in_p {
                    let mut text = e.unescape().unwrap_or_default().into_owned();
                    if !text.is_empty() {
                        // Detect synthetic bullet points injected by marksmen-docx mapping
                        if text.starts_with("•     ") {
                            let pad = if !output.ends_with("\n") && !output.ends_with("\n\n") { "\n- " } else { "- " };
                            text = text.replacen("•     ", pad, 1);
                        }

                        let mut formatted = text;
                        // Only apply formatting if it isn't raw math, as our source math already has notation
                        if is_math {
                            // Trim the implicit spaces added by our exporter `format!(" {} ", latex)`
                            if p_aligned_center {
                                formatted = format!("$$\n{}\n$$", formatted.trim());
                            } else {
                                formatted = format!("${}$", formatted.trim());
                            }
                            output.push_str(&formatted);
                        } else {
                            // Enable composite typographic rendering (e.g., *`code`*)
                            // Critically, we must pull leading/trailing whitespace OUTSIDE the emphasis 
                            // wrappers so adjacent formatting loops do not collide into false syntax (e.g., * * * = ** *)
                            let lead_chars = formatted.len() - formatted.trim_start_matches(' ').len();
                            let trail_chars = formatted.len() - formatted.trim_end_matches(' ').len();
                            
                            let mut core_text = formatted.trim().to_string();
                            if !core_text.is_empty() {
                                if is_code { core_text = format!("`{}`", core_text); }
                                if is_bold { core_text = format!("**{}**", core_text); }
                                if is_italic { core_text = format!("*{}*", core_text); }
                                if is_underline { core_text = format!("<u>{}</u>", core_text); }
                                if is_subscript { core_text = format!("<sub>{}</sub>", core_text); }
                                if is_superscript { core_text = format!("<sup>{}</sup>", core_text); }
                            }
                            formatted = format!("{}{}{}", " ".repeat(lead_chars), core_text, " ".repeat(trail_chars));
                            output.push_str(&formatted);
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
    
    Ok(cleaned)
}

