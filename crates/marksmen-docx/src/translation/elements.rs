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
    pub has_runs: bool,
}

/// Applies Markdown formatting tags to a running DOCX Paragraph
pub fn handle_event<'a>(
    event: Event<'a>, 
    doc: &mut Docx, 
    current_paragraph: &mut Paragraph,
    text_state: &mut TextState,
    in_blockquote: bool,
) {
    match event {
        // --- Structural Elements ---
        Event::Start(Tag::Heading { level, .. }) => {
            if text_state.has_runs {
                let p = std::mem::replace(current_paragraph, Paragraph::new());
                *doc = doc.clone().add_paragraph(p);
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
                *doc = doc.clone().add_paragraph(p);
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
            let tag = html.as_ref().to_lowercase();
            if tag.starts_with("<u") { text_state.is_underline = true; }
            else if tag.starts_with("</u") { text_state.is_underline = false; }
            else if tag.starts_with("<sub") { text_state.is_subscript = true; }
            else if tag.starts_with("</sub") { text_state.is_subscript = false; }
            else if tag.starts_with("<sup") { text_state.is_superscript = true; }
            else if tag.starts_with("</sup") { text_state.is_superscript = false; }
            else if tag.starts_with("<br") {
                *current_paragraph = current_paragraph.clone().add_run(Run::new().add_break(BreakType::TextWrapping));
                text_state.has_runs = true;
            }
            else if tag.contains("pagebreak") {
                *current_paragraph = current_paragraph.clone().add_run(Run::new().add_break(BreakType::Page));
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
            *current_paragraph = current_paragraph.clone().add_run(run);
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
            *current_paragraph = current_paragraph.clone().add_run(run);
            text_state.has_runs = true;
        }
        // Paragraph boundary (not Item — Item is flushed by document.rs)
        Event::End(TagEnd::Paragraph) => {
            if text_state.has_runs {
                let mut p = std::mem::replace(current_paragraph, Paragraph::new());
                if in_blockquote {
                    p = p.style("Quote");
                }
                *doc = doc.clone().add_paragraph(p);
                text_state.has_runs = false;
            }
        }
        Event::SoftBreak => {
            *current_paragraph = current_paragraph.clone().add_run(Run::new().add_text(" "));
            text_state.has_runs = true;
        }
        Event::HardBreak => {
            *current_paragraph = current_paragraph.clone().add_run(Run::new().add_break(BreakType::TextWrapping));
            text_state.has_runs = true;
        }
        _ => {}
    }
}
