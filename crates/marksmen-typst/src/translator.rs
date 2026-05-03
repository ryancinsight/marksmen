//! Main event-to-Typst dispatcher.
//!
//! Iterates over `pulldown-cmark` events and builds a Typst markup string
//! representing the entire document. The generated markup includes:
//!
//! - Page setup (`#set page(...)`)
//! - Text styling (`#set text(...)`)
//! - All markdown elements translated to Typst equivalents
//! - Math expressions translated from LaTeX to Typst syntax

use anyhow::Result;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};

use super::elements;
use super::math;
use marksmen_core::config::Config;

/// Translate a stream of pulldown-cmark events into a Typst markup string.
///
/// The output is a complete Typst source document ready for compilation,
/// including page setup preamble and all content.
pub fn translate(events: Vec<Event<'_>>, config: &Config) -> Result<String> {
    let mut output = String::with_capacity(4096);

    // Emit Typst preamble: page setup, text defaults.
    emit_preamble(&mut output, config);

    let mut state = TranslatorState::default();

    for event in events {
        translate_event(&mut output, &mut state, event, config);
    }

    if state.in_html_table {
        let buffered = std::mem::take(&mut state.html_table_buffer);
        process_html_blob(&buffered, &mut output, &mut state);
    }

    // Emit accumulated header/footer as Typst page overrides.
    if !state.header_buffer.is_empty() || !state.footer_buffer.is_empty() {
        let mut page_update = String::from("\n#set page(\n");
        if !state.header_buffer.is_empty() {
            let hdr = state.header_buffer.trim().replace('"', "''");
            page_update.push_str(&format!("  header: context [{}],\n", hdr));
        }
        if !state.footer_buffer.is_empty() {
            let ftr = state.footer_buffer.trim().replace('"', "''");
            page_update.push_str(&format!("  footer: context [{}],\n", ftr));
        }
        page_update.push_str(")\n");
        // Insert after the preamble, before body content.
        // Find the first blank line after preamble to insert.
        if let Some(pos) = output.find("\n\n") {
            output.insert_str(pos + 2, &page_update);
        } else {
            output.push_str(&page_update);
        }
    }

    Ok(output)
}

/// Alignment state for nested HTML div blocks.
#[derive(Default, Clone, Copy, PartialEq)]
enum HtmlAlign {
    #[default]
    None,
    Center,
    Indent(f32),
}

/// Internal state tracked across events.
#[derive(Default)]
struct TranslatorState {
    /// Whether we're inside an emphasis span.
    in_emphasis: bool,
    /// Whether we're inside a strong span.
    in_strong: bool,
    /// Whether we're inside a strikethrough span.
    in_strikethrough: bool,
    /// Whether we are inside any code block (fenced or indented).
    in_code_block: bool,
    /// Current code block language (if inside a fenced code block).
    code_block_lang: Option<String>,
    /// Accumulated code block content.
    code_block_content: String,
    /// Whether we're inside a block quote.
    in_blockquote: bool,
    /// Current list nesting depth.
    list_depth: u32,
    /// Whether the current list is ordered.
    list_ordered: Vec<bool>,
    /// Current ordered list item number.
    list_item_number: Vec<u64>,
    /// Table column alignments (when inside a table).
    table_alignments: Vec<Alignment>,
    /// Whether we're in a table header row.
    in_table_header: bool,
    /// Current table row cells.
    table_row_cells: Vec<String>,
    /// Current cell content accumulator.
    current_cell: String,
    /// Whether we're accumulating into a table cell.
    in_table_cell: bool,
    /// Whether we are currently inside a fended mermaid block.
    in_mermaid_block: bool,
    /// The accumulated plain text of the mermaid block.
    current_mermaid_text: String,
    /// Stack of HTML div alignment states for proper nesting.
    html_align_stack: Vec<HtmlAlign>,
    /// Whether the current <p> tag has a #text(...) wrapper that needs closing.
    html_p_has_text_wrapper: bool,
    /// Whether an <a href> link is currently open.
    html_a_open: bool,
    /// Whether we're inside a <style> block (CSS to ignore).
    in_html_style_block: bool,
    /// Whether we are currently buffering an HTML table.
    in_html_table: bool,
    /// Buffer for accumulating HTML table fragments.
    html_table_buffer: String,
    /// Maximum computed columns for an HTML table blob.
    html_table_cols: usize,
    /// Whether we're inside a <header> block (content suppressed from body).
    in_header_block: bool,
    /// Whether we're inside a <footer> block (content suppressed from body).
    in_footer_block: bool,
    /// Accumulated header content.
    header_buffer: String,
    footer_buffer: String,

    html_span_stack: Vec<bool>,
    /// Whether we are inside a <redact> block.
    in_redact: bool,
}

fn emit_preamble(output: &mut String, config: &Config) {
    // Page setup.
    output.push_str(&format!(
        "#set page(width: {}, height: {}, margin: (top: {}, right: {}, bottom: {}, left: {}),\n",
        config.page.width,
        config.page.height,
        config.page.margin_top,
        config.page.margin_right,
        config.page.margin_bottom,
        config.page.margin_left,
    ));

    if let Some(ref header) = config.page.header {
        let header_typst: String = header
            .replace("{p}", "#counter(page).display()")
            .replace("{t}", "#counter(page).final().first()");
        output.push_str(&format!("  header: context [{}],\n", header_typst));
    }

    if let Some(ref footer) = config.page.footer {
        let footer_typst: String = footer
            .replace("{p}", "#counter(page).display()")
            .replace("{t}", "#counter(page).final().first()");
        output.push_str(&format!("  footer: context [{}],\n", footer_typst));
    } else if config.page.page_numbers {
        output.push_str(&format!("  numbering: \"1\",\n"));
    }

    output.push_str(")\n");

    // Text defaults.
    let font_size = config.page.font_size.as_deref().unwrap_or("11pt");
    let font_family = config.page.font_family.as_deref().unwrap_or("Arial");
    output.push_str(&format!(
        "#set text(font: \"{}\", size: {})\n",
        font_family, font_size
    ));

    // Code styling.
    output.push_str(
        "#show raw: set text(font: (\"Consolas\", \"Courier New\", \"monospace\"), size: 10pt)\n",
    );
    // Inline code: light grey highlight behind the text.
    output.push_str(
        "#show raw.where(block: false): it => highlight(fill: luma(245), extent: 1.5pt)[#it]\n",
    );
    // Block code: explicit grey box with padding and rounded corners.
    output.push_str("#show raw.where(block: true): it => block(\n");
    output.push_str("  fill: luma(246),\n");
    output.push_str("  inset: (x: 10pt, y: 8pt),\n");
    output.push_str("  radius: 3pt,\n");
    output.push_str("  width: 100%,\n");
    output.push_str("  breakable: false,\n");
    output.push_str(")[#it]\n");
    // Disable syntax-highlight colouring: all other formats render code monochrome,
    // so normalise Typst/PDF to match rather than diverge via theme colours.
    output.push_str("#set raw(theme: none)\n");

    // Heading styling.
    output.push_str("#set heading(numbering: none)\n");
    // Ensure headings inherit the document font and don't force a bold weight
    // unless the underlying markdown AST natively outputs bold spans!
    output.push_str(&format!(
        "#show heading: set text(weight: \"regular\", font: \"{}\")\n",
        font_family
    ));

    // Paragraph spacing.
    let line_spacing = config.page.line_spacing.as_deref().unwrap_or("1.2em");
    output.push_str(&format!(
        "#set par(justify: true, spacing: {})\n",
        line_spacing
    ));

    // List alignment and indent.
    output.push_str("#set enum(number-align: start, indent: 1em)\n");
    output.push_str("#set list(indent: 1em)\n");

    output.push('\n');

    // Optional Title Page for research articles.
    if !config.title.is_empty() {
        output.push_str(&format!(
            "#align(center)[#text(size: 20pt, weight: \"bold\")[{}]]\n",
            config.title
        ));
        if !config.author.is_empty() {
            output.push_str(&format!(
                "#v(1em)\n#align(center)[#text(size: 14pt)[{}]]\n",
                config.author
            ));
        }
        if !config.date.is_empty() {
            output.push_str(&format!(
                "#v(1em)\n#align(center)[#text(size: 12pt)[{}]]\n",
                config.date
            ));
        }
        if !config.abstract_text.is_empty() {
            output.push_str(&format!(
                "#v(2em)\n#align(center)[*Abstract*]\n#pad(x: 2em)[{}]\n",
                config.abstract_text
            ));
        }
        output.push_str("#pagebreak()\n");
    }
}

fn translate_event(
    output: &mut String,
    state: &mut TranslatorState,
    event: Event<'_>,
    config: &Config,
) {
    match event {
        // --- Block elements ---
        Event::Start(Tag::Heading { level, .. }) => {
            if !state.in_header_block && !state.in_footer_block {
                output.push('\n');
                let prefix = elements::heading_prefix(heading_level_to_u8(level));
                output.push_str(&prefix);
                output.push(' ');
            }
        }
        Event::End(TagEnd::Heading(_)) => {
            if !state.in_header_block && !state.in_footer_block {
                output.push('\n');
            }
        }

        Event::Start(Tag::Paragraph) => {
            if state.in_header_block || state.in_footer_block {
                // Suppressed: content accumulates into header/footer buffer.
            } else if state.in_blockquote {
                // Blockquote paragraphs are handled by the blockquote wrapper.
            } else if !state.in_table_cell && state.list_depth == 0 {
                output.push('\n');
            }
        }
        Event::End(TagEnd::Paragraph) => {
            if state.in_header_block || state.in_footer_block {
                // Suppressed.
            } else if !state.in_table_cell && state.list_depth == 0 {
                output.push('\n');
            }
        }

        Event::Start(Tag::BlockQuote(_)) => {
            state.in_blockquote = true;
            output.push_str("\n#block(inset: (left: 1em), stroke: (left: 2pt + gray))[\n");
        }
        Event::End(TagEnd::BlockQuote(_)) => {
            state.in_blockquote = false;
            output.push_str("]\n");
        }

        Event::Start(Tag::CodeBlock(kind)) => {
            state.in_code_block = true;
            state.code_block_lang = match &kind {
                CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                    let lang_str = lang.to_string();
                    if lang_str == "mermaid" {
                        state.in_mermaid_block = true;
                        state.current_mermaid_text.clear();
                    }
                    Some(lang_str)
                }
                _ => None,
            };
            state.code_block_content.clear();
        }
        Event::End(TagEnd::CodeBlock) => {
            state.in_code_block = false;
            if state.in_mermaid_block {
                state.in_mermaid_block = false;
                let mermaid_src = std::mem::take(&mut state.current_mermaid_text);

                output.push('\n');
                match marksmen_mermaid::rendering::typst_backend::mermaid_to_typst(&mermaid_src) {
                    Ok(rendered) => output.push_str(&rendered),
                    Err(e) => output.push_str(&format!(
                        "#rect(fill: red.lighten(80%), stroke: red)[*Mermaid Error:* {}]\n",
                        e
                    )),
                }
                output.push('\n');
            } else {
                let lang = state.code_block_lang.take();
                let code = std::mem::take(&mut state.code_block_content);
                let block = elements::code_block(lang.as_deref(), &code);
                output.push('\n');
                output.push_str(&block);
                output.push('\n');
            }
        }

        // --- Lists ---
        Event::Start(Tag::List(start_number)) => {
            state.list_depth += 1;
            let ordered = start_number.is_some();
            state.list_ordered.push(ordered);
            state.list_item_number.push(start_number.unwrap_or(1));
            output.push('\n');
        }
        Event::End(TagEnd::List(_)) => {
            state.list_depth = state.list_depth.saturating_sub(1);
            state.list_ordered.pop();
            state.list_item_number.pop();
        }

        Event::Start(Tag::Item) => {
            let indent = "  ".repeat(state.list_depth.saturating_sub(1) as usize);
            let is_ordered = state.list_ordered.last().copied().unwrap_or(false);
            if is_ordered {
                let num = state.list_item_number.last_mut().unwrap();
                output.push_str(&format!("{}{}. ", indent, num));
                *num += 1;
            } else {
                output.push_str(&format!("{}- ", indent));
            }
        }
        Event::End(TagEnd::Item) => {
            output.push('\n');
        }

        // --- Inline styles ---
        Event::Start(Tag::Emphasis) => {
            state.in_emphasis = true;
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            dest.push_str("#emph[");
        }
        Event::End(TagEnd::Emphasis) => {
            state.in_emphasis = false;
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            dest.push(']');
        }

        Event::Start(Tag::Strong) => {
            state.in_strong = true;
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            dest.push_str("#strong[");
        }
        Event::End(TagEnd::Strong) => {
            state.in_strong = false;
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            dest.push(']');
        }

        Event::Start(Tag::Strikethrough) => {
            state.in_strikethrough = true;
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            dest.push_str("#strike[");
        }
        Event::End(TagEnd::Strikethrough) => {
            state.in_strikethrough = false;
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            dest.push(']');
        }

        // --- Links and images ---
        Event::Start(Tag::Link {
            dest_url, title: _, ..
        }) => {
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            if dest_url.starts_with('#') && dest_url.len() > 1 {
                dest.push_str(&format!("#link(<{}>)[", &dest_url[1..]));
            } else {
                dest.push_str(&format!("#link(\"{}\")[", dest_url));
            }
        }
        Event::End(TagEnd::Link) => {
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            dest.push(']');
        }

        Event::Start(Tag::Image {
            dest_url, title, ..
        }) => {
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            dest.push_str(&format!("\n#image(\"{}\"", dest_url));
            if !title.is_empty() {
                dest.push_str(&format!(", alt: \"{}\"", title));
            }
            dest.push_str(")\n");
        }
        Event::End(TagEnd::Image) => {
            // Image content is self-contained in Start.
        }

        // --- Tables ---
        Event::Start(Tag::Table(alignments)) => {
            state.table_alignments = alignments.to_vec();
            let ncols = state.table_alignments.len();
            let col_spec: Vec<&str> = state
                .table_alignments
                .iter()
                .map(|a| match a {
                    Alignment::Left => "left",
                    Alignment::Center => "center",
                    Alignment::Right => "right",
                    Alignment::None => "auto",
                })
                .collect();

            output.push_str("\n#align(center)[\n#table(\n");

            let mut frs = Vec::new();
            for _ in 0..ncols {
                frs.push("1fr");
            }

            output.push_str(&format!("  columns: ({}),\n", frs.join(", ")));
            output.push_str("  inset: 3pt,\n");
            output.push_str(&format!("  align: ({}),\n", col_spec.join(", ")));
        }
        Event::End(TagEnd::Table) => {
            output.push_str(")\n]\n");
            state.table_alignments.clear();
        }

        Event::Start(Tag::TableHead) => {
            state.in_table_header = true;
            state.table_row_cells.clear();
        }
        Event::End(TagEnd::TableHead) => {
            state.in_table_header = false;
            format_table_cells(&state.table_row_cells, true, output);
            state.table_row_cells.clear();
        }

        Event::Start(Tag::TableRow) => {
            state.table_row_cells.clear();
        }
        Event::End(TagEnd::TableRow) => {
            format_table_cells(&state.table_row_cells, false, output);
            state.table_row_cells.clear();
        }

        Event::Start(Tag::TableCell) => {
            state.in_table_cell = true;
            state.current_cell.clear();
        }
        Event::End(TagEnd::TableCell) => {
            state.in_table_cell = false;
            let cell = std::mem::take(&mut state.current_cell);
            state.table_row_cells.push(cell);
        }

        // --- Text content ---
        Event::Text(text) => {
            if state.in_redact {
                // Destructive redaction: physically replace the text bytes with blocks
                // to permanently scrub the sensitive data from the AST.
                let redacted_len = text.chars().count();
                let scrubbed = "█".repeat(redacted_len);
                let output_text = format!("#highlight(fill: black, extent: 1pt)[#text(fill: black)[{}]]", scrubbed);
                if state.in_table_cell {
                    state.current_cell.push_str(&output_text);
                } else {
                    output.push_str(&output_text);
                }
            } else if state.in_header_block {
                state.header_buffer.push_str(&elements::escape_text(&text));
            } else if state.in_footer_block {
                state.footer_buffer.push_str(&elements::escape_text(&text));
            } else if state.in_mermaid_block {
                state.current_mermaid_text.push_str(&text);
            } else if state.in_table_cell {
                state.current_cell.push_str(&elements::escape_text(&text));
            } else if state.in_code_block {
                // Inside a code block — don't escape.
                state.code_block_content.push_str(&text);
            } else {
                output.push_str(&elements::escape_text(&text));
            }
        }

        Event::Code(code) => {
            let formatted = elements::inline_code(&code);
            if state.in_table_cell {
                state.current_cell.push_str(&formatted);
            } else {
                output.push_str(&formatted);
            }
        }

        Event::SoftBreak => {
            if state.in_table_cell {
                state.current_cell.push(' ');
            } else {
                output.push('\n');
            }
        }

        Event::HardBreak => {
            if state.in_table_cell {
                state.current_cell.push_str(" \\ ");
            } else {
                output.push_str(" \\\n");
            }
        }

        Event::Rule => {
            output.push_str("\n#v(0.2em)\n#line(length: 100%, stroke: 0.5pt)\n#v(0.2em)\n");
        }

        // --- Math ---
        Event::InlineMath(latex_src) => {
            if config.math.enabled {
                let typst_math = math::latex_to_typst(&latex_src);
                if state.in_table_cell {
                    state.current_cell.push_str(&format!("${}$", typst_math));
                } else {
                    output.push_str(&format!("${}$", typst_math));
                }
            } else {
                // Math disabled — emit raw delimiters.
                let text = format!("${}$", latex_src);
                if state.in_table_cell {
                    state.current_cell.push_str(&text);
                } else {
                    output.push_str(&text);
                }
            }
        }

        Event::DisplayMath(latex_src) => {
            if config.math.enabled {
                let typst_math = math::latex_to_typst(&latex_src);
                output.push_str(&format!("\n$ {} $\n", typst_math));
            } else {
                output.push_str(&format!("\n$${}$$\n", latex_src));
            }
        }

        // --- Footnotes (pass-through for now) ---
        Event::FootnoteReference(label) => {
            output.push_str(&format!("#footnote[{}]", label));
        }
        Event::Start(Tag::FootnoteDefinition(label)) => {
            output.push_str(&format!("// Footnote: {}\n", label));
        }
        Event::End(TagEnd::FootnoteDefinition) => {}

        // --- Task lists ---
        Event::TaskListMarker(checked) => {
            if checked {
                output.push_str("[x] ");
            } else {
                output.push_str("[ ] ");
            }
        }

        // --- HTML (structured subsets) ---
        // pulldown-cmark may deliver block-level HTML as a single Event::Html blob
        // containing multiple tags plus inter-tag text (e.g. `<p style="...">content</p>`).
        // We split each blob into individual tag/text segments and process sequentially.
        Event::Html(html) | Event::InlineHtml(html) => {
            let lower = html.to_lowercase();
            if state.in_html_table {
                state.html_table_buffer.push_str(&html);
                if lower.contains("</table") {
                    state.in_html_table = false;
                    let buffered = std::mem::take(&mut state.html_table_buffer);
                    process_html_blob(&buffered, output, state);
                }
            } else if lower.contains("<table") {
                if lower.contains("</table") {
                    // Entire table is in one blob.
                    process_html_blob(&html, output, state);
                } else {
                    state.in_html_table = true;
                    state.html_table_buffer.push_str(&html);
                }
            } else {
                process_html_blob(&html, output, state);
            }
        }

        // Catch-all for any unhandled events.
        _ => {}
    }
}

/// Extract a CSS property value from an inline style string.
///
/// Given `lower` = `<p style="font-size: 14pt; margin: 0;">` and `property` = `"font-size:"`,
/// returns `Some("14pt")`.
fn extract_css_value(lower: &str, property: &str) -> Option<String> {
    if let Some(start) = lower.find(property) {
        let rest = &lower[start + property.len()..];
        let rest = rest.trim_start();
        let end = rest
            .find(';')
            .or_else(|| rest.find('"'))
            .or_else(|| rest.find('<'))
            .unwrap_or(rest.len());
        let value = rest[..end].trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// Escape characters that have special meaning in Typst markup.
///
/// In Typst, `#` starts a code expression, `@` starts a reference,
/// and `$` enters math mode. We escape these when emitting plain
/// text extracted from HTML blocks.
fn escape_typst_text(text: &str) -> String {
    text.replace('#', "\\#")
        .replace('@', "\\@")
        .replace('*', "\\*")
        .replace('_', "\\_")
}

/// Process a raw HTML blob from pulldown-cmark by splitting it into individual
/// `<tag>` and text segments. Block-level HTML may contain multiple tags and
/// inter-tag text in a single `Event::Html` delivery.
///
/// Strategy: split on `<` boundaries. Each segment is either a full tag `<...>`
/// possibly followed by trailing text, or a pure text run. We process each tag
/// individually, emitting its Typst equivalent, then emit any trailing plain text.
fn process_html_blob(html: &str, output: &mut String, state: &mut TranslatorState) {
    // Skip content entirely if inside a <style> block.
    if state.in_html_style_block {
        if html.to_lowercase().contains("</style") {
            state.in_html_style_block = false;
        }
        return;
    }

    let lower_html = html.to_lowercase();
    if lower_html.contains("<table") {
        let mut max_cols = 0;
        for tr in lower_html.split("<tr").skip(1) {
            let tr_body = if let Some(idx) = tr.find("</tr") {
                &tr[..idx]
            } else {
                tr
            };
            let th_count = tr_body
                .matches("<th")
                .count()
                .saturating_sub(tr_body.matches("<thead").count());
            let td_count = tr_body.matches("<td").count();
            let cols = th_count + td_count;
            if cols > max_cols {
                max_cols = cols;
            }
        }
        state.html_table_cols = max_cols.max(1);
    }

    // Split into segments on '<'. The first segment (before any '<') is leading text.
    let parts: Vec<&str> = html.split('<').collect();

    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            // Leading text before any tag — emit as-is if non-empty.
            if !part.is_empty() {
                let escaped = escape_typst_text(part);
                let dest = if state.in_table_cell {
                    &mut state.current_cell
                } else {
                    &mut *output
                };
                dest.push_str(&escaped);
            }
            continue;
        }

        // Reconstruct the tag: `<{part}`. The content after `>` is trailing text.
        let full = format!("<{}", part);
        let (tag_str, trailing_text) = if let Some(gt_pos) = full.find('>') {
            (&full[..gt_pos + 1], &full[gt_pos + 1..])
        } else {
            (full.as_str(), "")
        };

        let lower_tag = tag_str.to_lowercase();
        process_single_tag(&lower_tag, tag_str, &mut *output, state);

        // Emit any trailing text after the tag.
        if !trailing_text.is_empty() {
            let escaped = escape_typst_text(trailing_text);
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                &mut *output
            };
            dest.push_str(&escaped);
        }
    }
}

/// Process a single HTML tag and emit its Typst equivalent.
fn process_single_tag(
    lower: &str,
    original: &str,
    output: &mut String,
    state: &mut TranslatorState,
) {
    // --- <br> ---
    if lower.starts_with("<br") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("#linebreak()");

    // --- <sub> / </sub> ---
    } else if lower.starts_with("<sub") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("#sub[");
    } else if lower.starts_with("</sub") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push(']');

    // --- <sup> / </sup> ---
    } else if lower.starts_with("<sup") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("#super[");
    } else if lower.starts_with("</sup") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push(']');

    // --- <redact> / </redact> ---
    } else if lower.starts_with("<redact") {
        state.in_redact = true;
    } else if lower.starts_with("</redact") {
        state.in_redact = false;

    // --- <form> ---
    } else if lower.starts_with("<form") {
        let form_type = extract_css_value(lower, "type=\"").unwrap_or_else(|| "text".to_string());
        let form_name = extract_css_value(lower, "name=\"").unwrap_or_else(|| "field".to_string());
        let placeholder = format!(" [FORM: {} ({})] ", form_name.replace('\"', ""), form_type.replace('\"', ""));
        
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str(&format!("#box(stroke: 0.5pt + gray, inset: 4pt, fill: luma(240), baseline: 20%)[#text(size: 8pt, fill: gray)[{}]]", placeholder));
    }

    // --- <cite> (Stage 2) ---
    else if lower.starts_with("<cite") {
        if let Some(id) = extract_attr(original, "data-id") {
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            dest.push_str(&format!("#cite(\"{}\")", id));
        }
    } else if lower.starts_with("</cite>") {
        // Handled silently since `#cite(...)` doesn't wrap content
    } else if lower.starts_with("<b")

        && !lower.starts_with("<br")
        && !lower.starts_with("<body")
        && !lower.starts_with("<block")
        || lower.starts_with("<strong")
    {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("#strong[");
    } else if lower.starts_with("</b")
        && !lower.starts_with("</body")
        && !lower.starts_with("</block")
        || lower.starts_with("</strong")
    {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push(']');
    } else if lower.starts_with("<i")
        && !lower.starts_with("<img")
        && !lower.starts_with("<ins")
        && !lower.starts_with("<iframe")
        || lower.starts_with("<em")
    {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("#emph[");
    } else if lower.starts_with("</i")
        && !lower.starts_with("</ins")
        && !lower.starts_with("</iframe")
        || lower.starts_with("</em")
    {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push(']');
    } else if lower.starts_with("<u") && !lower.starts_with("<ul") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("#underline[");
    } else if lower.starts_with("</u") && !lower.starts_with("</ul") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push(']');
    // --- <span> ---
    } else if lower.starts_with("<span") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        let font_size = extract_css_value(lower, "font-size:");
        if let Some(ref fs) = font_size {
            dest.push_str(&format!("#text(size: {})[", fs.trim()));
            state.html_span_stack.push(true);
        } else {
            state.html_span_stack.push(false);
        }
    } else if lower.starts_with("</span") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        if let Some(true) = state.html_span_stack.pop() {
            dest.push(']');
        }
    // --- <ins> / </ins> ---
    } else if lower.starts_with("<ins") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("#underline(stroke: blue)[#text(fill: blue)[");
    } else if lower.starts_with("</ins") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("]]");
    // --- <del> / </del> ---
    } else if lower.starts_with("<del") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("#strike(stroke: red)[#text(fill: red)[");
    } else if lower.starts_with("</del") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("]]");
    // --- <div> ---
    } else if lower.starts_with("<div") {
        if lower.contains("page-break") {
            output.push_str("\n\n#pagebreak()\n\n");
        } else if lower.contains("align=\"center\"")
            || lower.contains("text-align: center")
            || lower.contains("text-align:center")
        {
            state.html_align_stack.push(HtmlAlign::Center);
            output.push_str("\n\n#align(center)[\n");
        } else if let Some(indent_str) = extract_css_value(lower, "margin-left:") {
            let indent_val = indent_str
                .replace("pt", "")
                .replace("px", "")
                .trim()
                .parse::<f32>()
                .unwrap_or(0.0);
            if indent_val > 0.0 {
                state.html_align_stack.push(HtmlAlign::Indent(indent_val));
                output.push_str(&format!("\n\n#pad(left: {}pt)[\n", indent_val));
            } else {
                state.html_align_stack.push(HtmlAlign::None);
                output.push_str("\n\n");
            }
        } else {
            state.html_align_stack.push(HtmlAlign::None);
            output.push_str("\n\n");
        }
    } else if lower.starts_with("</div") {
        match state.html_align_stack.pop() {
            Some(HtmlAlign::Center) | Some(HtmlAlign::Indent(_)) => {
                output.push_str("]\n\n");
            }
            _ => {
                output.push_str("\n\n");
            }
        }

    // --- <p> with optional font-size ---
    } else if lower.starts_with("<p") {
        output.push_str("\n\n");
        let font_size = extract_css_value(lower, "font-size:");
        if let Some(ref fs) = font_size {
            output.push_str(&format!("#text(size: {})[", fs.trim()));
            state.html_p_has_text_wrapper = true;
        } else {
            state.html_p_has_text_wrapper = false;
        }
    } else if lower.starts_with("</p") {
        if state.html_p_has_text_wrapper {
            output.push_str("]");
            state.html_p_has_text_wrapper = false;
        }
        output.push_str("\n\n");

    // --- <h1> through <h3> ---
    } else if lower.starts_with("<h1") {
        let font_size = extract_css_value(lower, "font-size:");
        let size = font_size.unwrap_or_else(|| "24pt".to_string());
        output.push_str(&format!(
            "\n\n#text(size: {}, weight: \"bold\")[",
            size.trim()
        ));
    } else if lower.starts_with("</h1") {
        output.push_str("]\n\n");
    } else if lower.starts_with("<h2") {
        output.push_str("\n== ");
    } else if lower.starts_with("</h2") || lower.starts_with("</h3") {
        output.push('\n');
    } else if lower.starts_with("<h3") {
        output.push_str("\n=== ");

    // --- <strong> / <b> ---
    } else if lower.starts_with("<strong") || lower.starts_with("<b>") || lower.starts_with("<b ") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("#strong[");
    } else if lower.starts_with("</strong") || lower.starts_with("</b>") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push(']');

    // --- <em> / <i> ---
    } else if lower.starts_with("<em") || lower.starts_with("<i>") || lower.starts_with("<i ") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("#emph[");
    } else if lower.starts_with("</em") || lower.starts_with("</i>") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push(']');

    // --- <span color> ---
    } else if lower.starts_with("<span") && lower.contains("color:") {
        if let Some(start) = lower.find("color:") {
            let rest = original[start + 6..].trim();
            if let Some(end) = rest
                .find('"')
                .or_else(|| rest.find(';'))
                .or_else(|| rest.find('<'))
            {
                let color = rest[..end].trim();
                let dest = if state.in_table_cell {
                    &mut state.current_cell
                } else {
                    output
                };
                dest.push_str(&format!("#text(fill: rgb(\"{}\"))[", color));
            }
        }
    } else if lower.starts_with("</span") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push(']');

    // --- <mark> (Comments) ---
    } else if lower.starts_with("<mark") && lower.contains("comment") {
        let _content = extract_attr_from_lower(lower, "data-content").unwrap_or_default();
        let _author =
            extract_attr_from_lower(lower, "data-author").unwrap_or_else(|| "Author".to_string());
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        // We only render the visual anchor; PDF annotation metadata is handled upstream.
        dest.push_str(&format!("#highlight(fill: yellow.lighten(60%))[{}]", "{"));
    } else if lower.starts_with("</mark") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("}");

    // --- <a> with id or href ---
    } else if lower.starts_with("<a") {
        if lower.contains("id=\"") {
            if let Some(s) = lower.find("id=\"") {
                let rest = &original[s + 4..];
                if let Some(e) = rest.find('"') {
                    let label = &rest[..e];
                    output.push_str(&format!("#h(0pt) <{}>\n", label));
                }
            }
        } else if lower.contains("href=\"") {
            if let Some(s) = original.find("href=\"") {
                let rest = &original[s + 6..];
                if let Some(e) = rest.find('"') {
                    let href = &rest[..e];
                    // Extract link text from between > and </a> if present in the same tag vicinity
                    let link_text = if let Some(gt) = original.find('>') {
                        let after = &original[gt + 1..];
                        if let Some(lt) = after.find('<') {
                            after[..lt].trim()
                        } else {
                            after.trim()
                        }
                    } else {
                        ""
                    };
                    if !link_text.is_empty() {
                        output.push_str(&format!("#link(\"{}\")[{}]", href, link_text));
                    } else {
                        output.push_str(&format!("#link(\"{}\")[", href));
                        state.html_a_open = true;
                    }
                }
            }
        }
    } else if lower.starts_with("</a") {
        if state.html_a_open {
            output.push(']');
            state.html_a_open = false;
        }

    // --- <img> ---
    } else if lower.starts_with("<img") {
        let mut src = "";
        let mut alt = "";
        let mut width = "";

        if let Some(s) = original.find("src=\"") {
            let rest = &original[s + 5..];
            if let Some(e) = rest.find('"') {
                src = &rest[..e];
            }
        }
        if let Some(s) = original.find("alt=\"") {
            let rest = &original[s + 5..];
            if let Some(e) = rest.find('"') {
                alt = &rest[..e];
            }
        }
        if let Some(s) = original.find("width=\"") {
            let rest = &original[s + 7..];
            if let Some(e) = rest.find('"') {
                width = rest[..e].trim();
            }
        } else {
            // Check for width in CSS style, avoiding max-width:
            let style_start = original
                .find("style=\"")
                .or_else(|| original.find("style='"));
            if let Some(s) = style_start {
                let rest = &original[s + 7..];
                if let Some(e) = rest.find('"').or_else(|| rest.find('\'')) {
                    let style_str = &rest[..e];
                    for rule in style_str.split(';') {
                        let rule = rule.trim();
                        if rule.starts_with("width:") {
                            width = rule[6..].trim();
                            break;
                        }
                    }
                }
            }
        }

        if !src.is_empty() {
            let dest = if state.in_table_cell {
                &mut state.current_cell
            } else {
                output
            };
            dest.push_str(&format!("\n#image(\"{}\"", src));
            if !width.is_empty() {
                dest.push_str(&format!(", width: {}", width));
            }
            if !alt.is_empty() {
                dest.push_str(&format!(", alt: \"{}\"", alt));
            }
            dest.push_str(")\n");
        }

    // --- <!-- pagebreak --> ---
    } else if lower.starts_with("<!-- pagebreak -->") {
        output.push_str("\n#pagebreak()\n");

    // --- Table formatting comments ---
    } else if lower.starts_with("<!-- colspan -->") || original.starts_with("<!-- COLSPAN") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("<!-- COLSPAN -->");
    } else if lower.starts_with("<!-- bg_color:") || original.starts_with("<!-- BG_COLOR") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str(original);

    // --- <style> ---
    } else if lower.starts_with("<style") {
        state.in_html_style_block = true;
    } else if lower.starts_with("</style") {
        state.in_html_style_block = false;

    // --- HTML Tables ---
    } else if lower.starts_with("<table") {
        let cols = if state.html_table_cols > 0 {
            state.html_table_cols
        } else {
            1
        };
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        // For dense tables (6+ cols), reduce font size to prevent overflow.
        if cols >= 6 {
            dest.push_str(&format!(
                "\n\n#align(center)[\n#set text(size: 6pt)\n#table(\n  columns: (1fr,) * {},\n  inset: 3pt,\n  stroke: 0.5pt + luma(200),\n",
                cols
            ));
        } else {
            dest.push_str(&format!(
                "\n\n#align(center)[\n#table(\n  columns: (1fr,) * {},\n  inset: 3pt,\n  stroke: 0.5pt + luma(200),\n",
                cols
            ));
        }
    } else if lower.starts_with("</table") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str(")\n]\n\n");
    } else if lower.starts_with("<tr") {
        // Typst tables wrap automatically by column count, so TR has no structural equivalent.
    } else if lower.starts_with("</tr") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push('\n');
    } else if lower.starts_with("<th") && !lower.starts_with("<thead") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("  [ #strong[");
    } else if lower.starts_with("</th") && !lower.starts_with("</thead") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("] ],\n");
    } else if lower.starts_with("<td") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("  [ ");
    } else if lower.starts_with("</td") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str(" ],\n");
    } else if lower.starts_with("<caption") {
        let cols = if state.html_table_cols > 0 {
            state.html_table_cols
        } else {
            1
        };
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str(&format!("  table.cell(colspan: {})[\n    ", cols));
    } else if lower.starts_with("</caption") {
        let dest = if state.in_table_cell {
            &mut state.current_cell
        } else {
            output
        };
        dest.push_str("\n  ],\n");
    // --- <header> / <footer> block suppression ---
    } else if lower.starts_with("<header") {
        state.in_header_block = true;
    } else if lower.starts_with("</header") {
        state.in_header_block = false;
    } else if lower.starts_with("<footer") {
        state.in_footer_block = true;
    } else if lower.starts_with("</footer") {
        state.in_footer_block = false;
    } else if lower.starts_with("<!-- page_num") {
        if state.in_header_block {
            state.header_buffer.push_str("#counter(page).display()");
        } else if state.in_footer_block {
            state.footer_buffer.push_str("#counter(page).display()");
        } else {
            output.push_str("#counter(page).display()");
        }
    } else if lower.starts_with("<!-- total_pages") {
        if state.in_header_block {
            state
                .header_buffer
                .push_str("#counter(page).final().first()");
        } else if state.in_footer_block {
            state
                .footer_buffer
                .push_str("#counter(page).final().first()");
        } else {
            output.push_str("#counter(page).final().first()");
        }
    } else if lower.starts_with("<thead")
        || lower.starts_with("</thead")
        || lower.starts_with("<tbody")
        || lower.starts_with("</tbody")
        || lower.starts_with("<code")
        || lower.starts_with("</code")
    {
        // Silently skip non-structural HTML table markup and inline code tags.
    } else {
        // Catch-all: skip silently to avoid noisy comments polluting Typst source.
    }
}

fn format_table_cells(cells: &[String], is_header: bool, output: &mut String) {
    let mut i = 0;
    while i < cells.len() {
        let mut cell_content = cells[i].clone();
        let mut colspan = 1;

        let mut bg_color = String::new();
        if let Some(start) = cell_content.find("<!-- BG_COLOR:") {
            if let Some(end) = cell_content[start..].find("-->") {
                let hex = cell_content[start + 14..start + end].trim();
                bg_color = format!("rgb(\"{}\")", hex);
                cell_content.replace_range(start..start + end + 3, "");
            }
        }

        let mut j = i + 1;
        while j < cells.len() && cells[j].contains("<!-- COLSPAN -->") {
            colspan += 1;
            j += 1;
        }

        let mut fill_attr = String::new();
        if !bg_color.is_empty() {
            fill_attr = format!("fill: {}, ", bg_color);
        }

        let content_fmt = if is_header {
            format!("*{}*", cell_content)
        } else {
            cell_content
        };

        if colspan > 1 || !bg_color.is_empty() {
            output.push_str(&format!(
                "  table.cell({}colspan: {})[ {} ],\n",
                fill_attr, colspan, content_fmt
            ));
        } else {
            output.push_str(&format!("  [ {} ],\n", content_fmt));
        }

        i += colspan;
    }
}

fn extract_attr_from_lower(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    if let Some(start) = tag.find(&needle) {
        let remaining = &tag[start + needle.len()..];
        if let Some(end) = remaining.find('"') {
            return Some(remaining[..end].to_string());
        }
    }
    None
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
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
#[cfg(test)]
mod tests {
    use super::*;
    use marksmen_core::parsing::parser::parse;

    fn translate_md(md: &str) -> String {
        let config = Config::default();
        let events = parse(md);
        translate(events, &config).unwrap()
    }

    #[test]
    fn heading_translation() {
        let result = translate_md("# Hello World");
        assert!(result.contains("= Hello World"));
    }

    #[test]
    fn emphasis_translation() {
        let result = translate_md("*italic*");
        // The Typst translator emits emphasis as #emph[…]
        assert!(
            result.contains("#emph["),
            "Expected '#emph[' in: {}",
            result
        );
        assert!(result.contains("italic"));
    }

    #[test]
    fn strong_translation() {
        let result = translate_md("**bold**");
        // The Typst translator emits strong as #strong[…]
        assert!(
            result.contains("#strong["),
            "Expected '#strong[' in: {}",
            result
        );
        assert!(result.contains("bold"));
    }

    #[test]
    fn inline_math_translation() {
        let result = translate_md("The equation $E = mc^2$ is famous.");
        // latex_to_typst inserts spaces between adjacent alphabetic identifiers
        // to prevent Typst from parsing `mc` as a single multi-letter variable.
        assert!(
            result.contains("$E = m c^2$"),
            "Expected Typst-math output in: {}",
            result
        );
    }

    #[test]
    fn display_math_translation() {
        let result = translate_md("$$\\frac{a}{b}$$");
        assert!(result.contains("frac(a, b)"));
    }

    #[test]
    fn code_block_translation() {
        let result = translate_md("```rust\nfn main() {}\n```");
        assert!(result.contains("```rust"));
        assert!(result.contains("fn main() {}"));
    }

    #[test]
    fn list_translation() {
        let result = translate_md("- Item 1\n- Item 2");
        // The Typst translator emits list items as `- #[…]`
        assert!(
            result.contains("Item 1"),
            "Expected 'Item 1' in: {}",
            result
        );
        assert!(
            result.contains("Item 2"),
            "Expected 'Item 2' in: {}",
            result
        );
    }

    #[test]
    fn preamble_includes_page_setup() {
        let result = translate_md("# Test");
        assert!(result.contains("#set page("));
        assert!(result.contains("#set text("));
    }
}
