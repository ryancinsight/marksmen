use docx_rs::*;
use pulldown_cmark::{Event, Tag, TagEnd};

#[derive(Default, Debug, Clone)]
pub struct TextState {
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_code: bool,
    pub is_underline: bool,
    pub is_subscript: bool,
    pub is_superscript: bool,
    pub is_ins: bool,
    pub is_del: bool,
    pub revision_author: Option<String>,
    pub revision_date: Option<String>,
    pub is_highlight: bool,
    pub has_runs: bool,
    pub comment_id_counter: usize,
    pub active_comment_id: Option<usize>,
    pub in_header: bool,
    pub in_footer: bool,
    /// Optional lookup table built from the source DOCX `comments.xml`.
    /// Maps normalized comment content → original `w:id` value (numeric).
    /// When populated, the comment handler uses the original ID instead of
    /// the auto-incremented counter, so `w:commentRangeStart` in `document.xml`
    /// matches the verbatim-passed-through `comments.xml`.
    pub source_comment_ids: std::collections::HashMap<String, usize>,
}

pub enum Container<'a> {
    Doc(&'a mut Docx),
    Header(&'a mut Header),
    Footer(&'a mut Footer)
}

impl<'a> Container<'a> {
    pub fn add_paragraph(mut self, p: Paragraph) -> Self {
        match self {
            Self::Doc(ref mut d) => { **d = d.clone().add_paragraph(p); self }
            Self::Header(ref mut h) => { **h = h.clone().add_paragraph(p); self }
            Self::Footer(ref mut f) => { **f = f.clone().add_paragraph(p); self }
        }
    }
    
    pub fn add_table(mut self, t: Table) -> Self {
        match self {
            Self::Doc(ref mut d) => { **d = d.clone().add_table(t); self }
            Self::Header(ref mut h) => { **h = h.clone().add_table(t); self }
            Self::Footer(ref mut f) => { **f = f.clone().add_table(t); self }
        }
    }
}

/// Applies Markdown formatting tags to a running DOCX Paragraph
pub fn handle_event<'a>(
    event: Event<'a>, 
    mut container: Container, 
    current_paragraph: &mut Paragraph,
    text_state: &mut TextState,
    in_blockquote: bool,
) {
    match event {
        // --- Structural Elements ---
        Event::Start(Tag::Heading { level, .. }) => {
            if text_state.has_runs {
                let p = std::mem::replace(current_paragraph, Paragraph::new());
                container = container.add_paragraph(p);
                text_state.has_runs = false;
            }
            let heading_style = match level {
                pulldown_cmark::HeadingLevel::H1 => "Heading1",
                pulldown_cmark::HeadingLevel::H2 => "Heading2",
                pulldown_cmark::HeadingLevel::H3 => "Heading3",
                pulldown_cmark::HeadingLevel::H4 => "Heading4",
                pulldown_cmark::HeadingLevel::H5 => "Heading5",
                pulldown_cmark::HeadingLevel::H6 => "Heading6",
            };
            *current_paragraph = Paragraph::new().style(heading_style);
        }
        Event::End(TagEnd::Heading(_level)) => {
            // Flush heading
            if text_state.has_runs {
                let p = std::mem::replace(current_paragraph, Paragraph::new());
                container = container.add_paragraph(p);
                text_state.has_runs = false;
            }
        }
        
        // Tag::List, Tag::Item, TagEnd::List, TagEnd::Item are handled in document.rs
        // via DOCX numbering properties — handled before this fallthrough.

        // --- Text Formatting Flags ---
        Event::Start(Tag::Strong) => text_state.is_bold = true,
        Event::End(TagEnd::Strong) => text_state.is_bold = false,
        Event::Start(Tag::Emphasis) => text_state.is_italic = true,
        Event::End(TagEnd::Emphasis) => text_state.is_italic = false,
        
        Event::Html(html) | Event::InlineHtml(html) => {
            let original_tag = html.as_ref();
            let tag = original_tag.to_lowercase();
            if tag.starts_with("<u") { text_state.is_underline = true; }
            else if tag.starts_with("</u") { text_state.is_underline = false; }
            else if tag.starts_with("<sub") { text_state.is_subscript = true; }
            else if tag.starts_with("</sub") { text_state.is_subscript = false; }
            else if tag.starts_with("<sup") { text_state.is_superscript = true; }
            else if tag.starts_with("</sup") { text_state.is_superscript = false; }
            else if tag.starts_with("<ins") {
                text_state.is_ins = true;
                text_state.revision_author = extract_attr(original_tag, "data-author");
                text_state.revision_date = extract_attr(original_tag, "data-date");
            }
            else if tag.starts_with("</ins") { text_state.is_ins = false; }
            else if tag.starts_with("<del") {
                text_state.is_del = true;
                text_state.revision_author = extract_attr(original_tag, "data-author");
                text_state.revision_date = extract_attr(original_tag, "data-date");
            }
            else if tag.starts_with("</del") { text_state.is_del = false; }
            else if tag.starts_with("<header") { text_state.in_header = true; }
            else if tag.starts_with("</header") { text_state.in_header = false; }
            else if tag.starts_with("<footer") { text_state.in_footer = true; }
            else if tag.starts_with("</footer") { text_state.in_footer = false; }
            else if tag.starts_with("<mark") && tag.contains("comment") {
                let author = extract_attr(original_tag, "data-author").unwrap_or_else(|| "Author".to_string());
                let content = extract_attr(original_tag, "data-content").unwrap_or_default();

                // Prefer original source ID to preserve comment anchor consistency with
                // the verbatim-passed-through comments.xml. Fall back to counter.
                let content_norm = content.trim().to_ascii_lowercase();
                let id = text_state.source_comment_ids
                    .get(&content_norm)
                    .copied()
                    .unwrap_or_else(|| {
                        let c = text_state.comment_id_counter;
                        text_state.comment_id_counter += 1;
                        c
                    });
                text_state.active_comment_id = Some(id);

                let comment = Comment::new(id)
                    .author(author)
                    .add_paragraph(Paragraph::new().add_run(Run::new().add_text(content)));

                *current_paragraph = current_paragraph.clone().add_comment_start(comment);
            }
            else if tag.starts_with("<mark") && tag.contains("highlight") {
                text_state.is_highlight = true;
            }
            else if tag.starts_with("<mark") && tag.contains("align-center") {
                *current_paragraph = current_paragraph.clone().align(AlignmentType::Center);
            }
            else if tag.starts_with("</mark") {
                if let Some(id) = text_state.active_comment_id.take() {
                    *current_paragraph = current_paragraph.clone().add_comment_end(id);
                }
                text_state.is_highlight = false;
            }
            else if tag.starts_with("<br") {
                *current_paragraph = current_paragraph.clone().add_run(Run::new().add_break(BreakType::TextWrapping));
                text_state.has_runs = true;
            }
            else if tag.contains("pagebreak") {
                *current_paragraph = current_paragraph.clone().add_run(Run::new().add_break(BreakType::Page));
                text_state.has_runs = true;
            }
            else if tag.contains("<!-- page:") || tag.starts_with("<!-- page:") {
                // Page geometry metadata comment from reader — consumed silently by the writer.
                // Page size/margins are applied by the outer convert() from the config or
                // from the parsed metadata section before the event loop.
            }
            else if tag.contains("<!-- page_num -->") {
                // Reconstruct w:fldChar PAGE field in the current paragraph
                let run_begin = Run::new().add_field_char(FieldCharType::Begin, false);
                let run_instr = Run::new().add_instr_text(InstrText::Unsupported(" PAGE  \\* Arabic  \\* MERGEFORMAT ".to_string()));
                let run_sep   = Run::new().add_field_char(FieldCharType::Separate, false);
                let run_disp  = Run::new().add_text("1".to_string());
                let run_end   = Run::new().add_field_char(FieldCharType::End, false);
                *current_paragraph = current_paragraph.clone()
                    .add_run(run_begin)
                    .add_run(run_instr)
                    .add_run(run_sep)
                    .add_run(run_disp)
                    .add_run(run_end);
                text_state.has_runs = true;
            }
            else if tag.contains("<!-- total_pages -->") {
                // Reconstruct w:fldSimple NUMPAGES field in the current paragraph
                let run_begin = Run::new().add_field_char(FieldCharType::Begin, false);
                let run_instr = Run::new().add_instr_text(InstrText::Unsupported(" NUMPAGES  \\* Arabic  \\* MERGEFORMAT ".to_string()));
                let run_sep   = Run::new().add_field_char(FieldCharType::Separate, false);
                let run_disp  = Run::new().add_text("1".to_string());
                let run_end   = Run::new().add_field_char(FieldCharType::End, false);
                *current_paragraph = current_paragraph.clone()
                    .add_run(run_begin)
                    .add_run(run_instr)
                    .add_run(run_sep)
                    .add_run(run_disp)
                    .add_run(run_end);
                text_state.has_runs = true;
            }
        }
        // --- Content Insertion ---
        Event::Text(text) => {
            let mut run = Run::new().add_text(text.to_string());
            if text_state.is_bold {
                run = run.bold();
            }
            if text_state.is_italic {
                run = run.italic();
            }
            if text_state.is_code {
                // Approximate inline code style (DOCX typically uses a monospaced font run)
                run = run.fonts(RunFonts::new().ascii("Consolas"));
            }
            if text_state.is_underline {
                run = run.underline("single");
            }
            if text_state.is_subscript {
                run.run_property = run.run_property.vert_align(VertAlignType::SubScript);
            }
            if text_state.is_superscript {
                run.run_property = run.run_property.vert_align(VertAlignType::SuperScript);
            }
            if text_state.is_highlight {
                run = run.highlight("yellow");
            }
            
            if text_state.is_ins {
                let mut ins = docx_rs::Insert::new(run);
                if let Some(author) = &text_state.revision_author {
                    ins = ins.author(author);
                }
                if let Some(date) = &text_state.revision_date {
                    ins = ins.date(date);
                }
                *current_paragraph = current_paragraph.clone().add_insert(ins);
            } else if text_state.is_del {
                let mut del = docx_rs::Delete::new().add_run(run);
                if let Some(author) = &text_state.revision_author {
                    del = del.author(author);
                }
                if let Some(date) = &text_state.revision_date {
                    del = del.date(date);
                }
                *current_paragraph = current_paragraph.clone().add_delete(del);
            } else {
                *current_paragraph = current_paragraph.clone().add_run(run);
            }
            text_state.has_runs = true;
        }
        Event::Code(code_text) => {
            // Inline code segment MUST inherit active structural states to preserve layout
            let mut run = Run::new()
                .add_text(code_text.to_string())
                .fonts(RunFonts::new().ascii("Consolas"));
            if text_state.is_bold { run = run.bold(); }
            if text_state.is_italic { run = run.italic(); }
            if text_state.is_underline { run = run.underline("single"); }
            if text_state.is_subscript { run.run_property = run.run_property.vert_align(VertAlignType::SubScript); }
            if text_state.is_superscript { run.run_property = run.run_property.vert_align(VertAlignType::SuperScript); }
            if text_state.is_highlight { run = run.highlight("yellow"); }
            
            if text_state.is_ins {
                let mut ins = docx_rs::Insert::new(run);
                if let Some(author) = &text_state.revision_author {
                    ins = ins.author(author);
                }
                if let Some(date) = &text_state.revision_date {
                    ins = ins.date(date);
                }
                *current_paragraph = current_paragraph.clone().add_insert(ins);
            } else if text_state.is_del {
                let mut del = docx_rs::Delete::new().add_run(run);
                if let Some(author) = &text_state.revision_author {
                    del = del.author(author);
                }
                if let Some(date) = &text_state.revision_date {
                    del = del.date(date);
                }
                *current_paragraph = current_paragraph.clone().add_delete(del);
            } else {
                *current_paragraph = current_paragraph.clone().add_run(run);
            }
            text_state.has_runs = true;
        }
        // Paragraph boundary (not Item — Item is flushed by document.rs)
        Event::End(TagEnd::Paragraph) => {
            if text_state.has_runs {
                let mut p = std::mem::replace(current_paragraph, Paragraph::new());
                if in_blockquote {
                    p = p.style("Quote");
                }
                // Header/footer paragraphs are suppressed from the document body.
                // The source_docx verbatim passthrough injects word/header1.xml and
                // word/footer1.xml directly into the ZIP, making body injection redundant
                // and causing duplicate text extraction in roundtrip similarity checks.
                if !text_state.in_header && !text_state.in_footer {
                    container = container.add_paragraph(p);
                }
                text_state.has_runs = false;
            }
        }
        Event::SoftBreak => {
            *current_paragraph = current_paragraph.clone().add_run(Run::new().add_break(BreakType::TextWrapping));
            text_state.has_runs = true;
        }
        Event::HardBreak => {
            *current_paragraph = current_paragraph.clone().add_run(Run::new().add_break(BreakType::TextWrapping));
            text_state.has_runs = true;
        }
        Event::Rule => {
            if text_state.has_runs {
                let p = std::mem::replace(current_paragraph, Paragraph::new());
                container = container.add_paragraph(p);
                text_state.has_runs = false;
            }
            let p = Paragraph::new().add_run(Run::new().add_break(BreakType::Page));
            container = container.add_paragraph(p);
        }
        _ => {}
    }
}

pub(crate) fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    if let Some(start) = tag.find(&needle) {
        let remaining = &tag[start + needle.len()..];
        if let Some(end) = remaining.find('"') {
            let extracted = &remaining[..end];
            let unescaped = extracted
                .replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&#39;", "'")
                .replace("&apos;", "'");
            return Some(unescaped);
        }
    }
    None
}
