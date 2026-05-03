//! Marp Markdown presentation writer for the marksmen workspace.
//!
//! # Output Contract
//!
//! Produces a self-contained [Marp](https://marp.app) Markdown document
//! renderable by `marp-cli` or the VS Code Marp extension.
//!
//! ## Slide Segmentation
//!
//! Slide boundaries are emitted in two cases:
//! 1. `Event::Rule` (`---` in the source) → literal `---` Marp separator.
//! 2. `Event::Start(Tag::Heading { level: H1 })` → `---` separator inserted
//!    before the heading for all H1s after the first slide, enabling
//!    the `headingDivider: 1` directive.
//!
//! ## Markdown Serialization
//!
//! pulldown-cmark events are mapped back to CommonMark syntax. Tables are
//! serialized as GitHub Flavored Markdown pipe tables. Inline math is
//! preserved as `$...$` and display math as `$$\n...\n$$` for MathJax/KaTeX
//! rendering in Marp (requires `math: katex` or `math: mathjax` directives).
//!
//! # Invariant
//! The round-trip `Markdown → marksmen-marp → marksmen-marp-read → Markdown`
//! preserves all visible plain-text content. Formatting fidelity is
//! best-effort since Marp Markdown IS CommonMark.

use anyhow::Result;
use marksmen_core::config::Config;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};

/// Convert a pulldown-cmark event stream into a Marp Markdown document.
pub fn convert(events: Vec<Event<'_>>, config: &Config) -> Result<String> {
    let mut out = String::with_capacity(events.len() * 32);

    // ── YAML front matter ────────────────────────────────────────────────────
    out.push_str("---\n");
    out.push_str("marp: true\n");
    out.push_str("headingDivider: 2\n");
    out.push_str("paginate: true\n");
    out.push_str("theme: default\n");
    if !config.title.is_empty() {
        out.push_str(&format!(
            "title: \"{}\"\n",
            config.title.replace('"', "\\\"")
        ));
    }
    if !config.author.is_empty() {
        out.push_str(&format!(
            "author: \"{}\"\n",
            config.author.replace('"', "\\\"")
        ));
    }
    if !config.date.is_empty() {
        out.push_str(&format!("date: \"{}\"\n", config.date.replace('"', "\\\"")));
    }
    out.push_str("math: katex\n");
    out.push_str("---\n\n");

    let mut state = SerState::default();

    for event in &events {
        match event {
            // ── Slide boundaries ──────────────────────────────────────────
            Event::Rule => {
                flush_inline(&mut state, &mut out);
                out.push_str("\n---\n\n");
            }

            // ── Headings ──────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                flush_inline(&mut state, &mut out);
                // H1 and H2 start a new Marp slide (headingDivider: 2 handles this
                // at render time, but we also emit explicit `---` separators
                // for readers that do not respect headingDivider).
                if (*level == HeadingLevel::H1 || *level == HeadingLevel::H2)
                    && state.heading_count > 0
                {
                    out.push_str("\n---\n\n");
                }
                state.in_heading = true;
                state.heading_prefix = heading_prefix(*level);
            }
            Event::End(TagEnd::Heading(_)) => {
                state.in_heading = false;
                state.heading_count += 1;
                flush_inline(&mut state, &mut out);
                out.push('\n');
                out.push('\n');
            }

            // ── Paragraphs ────────────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {
                state.in_paragraph = true;
            }
            Event::End(TagEnd::Paragraph) => {
                state.in_paragraph = false;
                flush_inline(&mut state, &mut out);
                out.push_str("\n\n");
            }

            // ── Block quotes ──────────────────────────────────────────────
            Event::Start(Tag::BlockQuote(_)) => {
                state.blockquote_depth += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                state.blockquote_depth = state.blockquote_depth.saturating_sub(1);
                out.push('\n');
            }

            // ── Code blocks ───────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) => {
                flush_inline(&mut state, &mut out);
                out.push_str("```");
                out.push_str(lang.as_ref());
                out.push('\n');
                state.in_code_block = true;
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush_inline(&mut state, &mut out);
                out.push_str("```\n");
                state.in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                state.in_code_block = false;
                out.push_str("```\n\n");
            }

            // ── Lists ─────────────────────────────────────────────────────
            Event::Start(Tag::List(start)) => {
                state.list_stack.push(start.map(|n| n as usize));
            }
            Event::End(TagEnd::List(_)) => {
                state.list_stack.pop();
                if state.list_stack.is_empty() {
                    out.push('\n');
                }
            }
            Event::Start(Tag::Item) => {
                flush_inline(&mut state, &mut out);
                let depth = state.list_stack.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                let prefix = match state.list_stack.last_mut() {
                    Some(Some(n)) => {
                        let p = format!("{}{}. ", indent, n);
                        *n += 1;
                        p
                    }
                    _ => format!("{}- ", indent),
                };
                state.pending_item_prefix = Some(prefix);
            }
            Event::End(TagEnd::Item) => {
                flush_inline(&mut state, &mut out);
                out.push('\n');
            }

            // ── Tables ────────────────────────────────────────────────────
            Event::Start(Tag::Table(alignments)) => {
                flush_inline(&mut state, &mut out);
                state.table_alignments = alignments.to_vec();
                state.in_table = true;
                state.table_row_idx = 0;
                state.table_cells.clear();
            }
            Event::End(TagEnd::Table) => {
                state.in_table = false;
                out.push('\n');
            }
            Event::Start(Tag::TableHead) => {
                state.in_table_head = true;
            }
            Event::End(TagEnd::TableHead) => {
                state.in_table_head = false;
                // Emit the header row and the separator row.
                emit_table_row(&state.table_cells, &mut out);
                emit_table_separator(&state.table_alignments, state.table_cells.len(), &mut out);
                state.table_cells.clear();
            }
            Event::Start(Tag::TableRow) => {
                state.table_cells.clear();
            }
            Event::End(TagEnd::TableRow) => {
                emit_table_row(&state.table_cells, &mut out);
                state.table_cells.clear();
                state.table_row_idx += 1;
            }
            Event::Start(Tag::TableCell) => {
                state.table_cells.push(String::new());
            }
            Event::End(TagEnd::TableCell) => {}

            // ── Inline formatting ─────────────────────────────────────────
            Event::Start(Tag::Strong) => state.bold_depth += 1,
            Event::End(TagEnd::Strong) => {
                state.bold_depth = state.bold_depth.saturating_sub(1);
            }
            Event::Start(Tag::Emphasis) => state.italic_depth += 1,
            Event::End(TagEnd::Emphasis) => {
                state.italic_depth = state.italic_depth.saturating_sub(1);
            }
            Event::Start(Tag::Strikethrough) => state.strikethrough = true,
            Event::End(TagEnd::Strikethrough) => state.strikethrough = false,
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                state.link_url = dest_url.as_ref().to_string();
                state.link_title = title.as_ref().to_string();
                state.in_link = true;
                state.link_text_buf.clear();
            }
            Event::End(TagEnd::Link) => {
                state.in_link = false;
                let text = std::mem::take(&mut state.link_text_buf);
                let link_md = if state.link_title.is_empty() {
                    format!("[{}]({})", text, state.link_url)
                } else {
                    format!("[{}]({} \"{}\")", text, state.link_url, state.link_title)
                };
                push_text(&mut state, &mut out, &link_md, false);
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                state.image_url = dest_url.as_ref().to_string();
                state.image_title = title.as_ref().to_string();
                state.image_alt_buf.clear();
                state.in_image = true;
            }
            Event::End(TagEnd::Image) => {
                state.in_image = false;
                let alt = std::mem::take(&mut state.image_alt_buf);
                let img = if state.image_title.is_empty() {
                    format!("![{}]({})", alt, state.image_url)
                } else {
                    format!("![{}]({} \"{}\")", alt, state.image_url, state.image_title)
                };
                push_text(&mut state, &mut out, &img, false);
            }

            // ── Inline code ───────────────────────────────────────────────
            Event::Code(text) => {
                push_text(&mut state, &mut out, &format!("`{}`", text), false);
            }

            // ── Text ──────────────────────────────────────────────────────
            Event::Text(text) => {
                let t = text.as_ref();
                if state.in_image {
                    state.image_alt_buf.push_str(t);
                } else if state.in_link {
                    state.link_text_buf.push_str(t);
                } else if state.in_code_block {
                    let bq = "> ".repeat(state.blockquote_depth);
                    for line in t.lines() {
                        out.push_str(&bq);
                        out.push_str(line);
                        out.push('\n');
                    }
                } else {
                    push_text(&mut state, &mut out, t, true);
                }
            }
            Event::SoftBreak => {
                push_text(&mut state, &mut out, "\n", false);
            }
            Event::HardBreak => {
                push_text(&mut state, &mut out, "  \n", false);
            }

            // ── Math ──────────────────────────────────────────────────────
            Event::InlineMath(math) => {
                push_text(&mut state, &mut out, &format!("${}$", math), false);
            }
            Event::DisplayMath(math) => {
                flush_inline(&mut state, &mut out);
                out.push_str("$$\n");
                out.push_str(math.as_ref());
                out.push_str("\n$$\n\n");
            }

            // ── Footnotes ─────────────────────────────────────────────────
            Event::FootnoteReference(label) => {
                push_text(&mut state, &mut out, &format!("[^{}]", label), false);
            }

            _ => {}
        }
    }

    Ok(out)
}

// ── State machine ────────────────────────────────────────────────────────────

#[derive(Default)]
struct SerState {
    in_heading: bool,
    heading_prefix: String,
    heading_count: usize,

    in_paragraph: bool,
    blockquote_depth: usize,
    in_code_block: bool,

    list_stack: Vec<Option<usize>>,
    pending_item_prefix: Option<String>,

    bold_depth: usize,
    italic_depth: usize,
    strikethrough: bool,

    in_link: bool,
    link_url: String,
    link_title: String,
    link_text_buf: String,

    in_image: bool,
    image_url: String,
    image_title: String,
    image_alt_buf: String,

    in_table: bool,
    in_table_head: bool,
    table_row_idx: usize,
    table_cells: Vec<String>,
    table_alignments: Vec<Alignment>,

    /// Inline text accumulator for the current span.
    inline_buf: String,
}

/// Wrap `text` with the currently active inline markers and append it either
/// to a table cell, the link text buffer, or the inline buffer.
fn push_text(state: &mut SerState, out: &mut String, text: &str, apply_fmt: bool) {
    let formatted = if apply_fmt {
        let mut s = text.to_string();
        if state.strikethrough {
            s = format!("~~{}~~", s);
        }
        if state.italic_depth > 0 {
            s = format!("*{}*", s);
        }
        if state.bold_depth > 0 {
            s = format!("**{}**", s);
        }
        s
    } else {
        text.to_string()
    };

    if state.in_table
        && let Some(cell) = state.table_cells.last_mut() {
            cell.push_str(&formatted);
            return;
        }
    if state.in_link {
        state.link_text_buf.push_str(text);
        return;
    }
    if state.in_heading {
        out.push_str(&state.heading_prefix.clone());
        state.in_heading = false; // prefix written once per heading
        state.heading_prefix.clear();
    }
    if let Some(prefix) = state.pending_item_prefix.take() {
        let bq = "> ".repeat(state.blockquote_depth);
        out.push_str(&bq);
        out.push_str(&prefix);
    } else if state.in_paragraph && state.inline_buf.is_empty() {
        let bq = "> ".repeat(state.blockquote_depth);
        if !bq.is_empty() {
            out.push_str(&bq);
        }
    }
    state.inline_buf.push_str(&formatted);
}

/// Flush the inline accumulator to `out` when a block-level boundary is reached.
fn flush_inline(state: &mut SerState, out: &mut String) {
    if !state.inline_buf.is_empty() {
        out.push_str(&std::mem::take(&mut state.inline_buf));
    }
    // Reset heading prefix if not yet consumed.
    if state.in_heading && !state.heading_prefix.is_empty() {
        out.push_str(&std::mem::take(&mut state.heading_prefix));
        state.in_heading = false;
    }
}

fn heading_prefix(level: HeadingLevel) -> String {
    let hashes = match level {
        HeadingLevel::H1 => "#",
        HeadingLevel::H2 => "##",
        HeadingLevel::H3 => "###",
        HeadingLevel::H4 => "####",
        HeadingLevel::H5 => "#####",
        HeadingLevel::H6 => "######",
    };
    format!("{} ", hashes)
}

fn emit_table_row(cells: &[String], out: &mut String) {
    out.push('|');
    for cell in cells {
        out.push(' ');
        out.push_str(cell);
        out.push_str(" |");
    }
    out.push('\n');
}

fn emit_table_separator(alignments: &[Alignment], col_count: usize, out: &mut String) {
    let count = col_count.max(alignments.len());
    out.push('|');
    for i in 0..count {
        let sep = match alignments.get(i).unwrap_or(&Alignment::None) {
            Alignment::Left => " :--- |",
            Alignment::Center => " :---: |",
            Alignment::Right => " ---: |",
            Alignment::None => " --- |",
        };
        out.push_str(sep);
    }
    out.push('\n');
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use marksmen_core::parsing::parser;

    #[test]
    fn test_front_matter_present() {
        let md = "# Hello\nBody text.";
        let events = parser::parse(md);
        let marp = convert(events, &Config::default()).unwrap();
        assert!(marp.starts_with("---\n"), "front matter missing");
        assert!(marp.contains("marp: true"), "marp directive missing");
        assert!(
            marp.contains("paginate: true"),
            "paginate directive missing"
        );
    }

    #[test]
    fn test_h1_slide_separator() {
        let md = "# First\nBody.\n\n# Second\nBody two.";
        let events = parser::parse(md);
        let marp = convert(events, &Config::default()).unwrap();
        // First H1 must not have a preceding separator (it IS the first slide).
        // Second H1 must have `---` inserted before it.
        let after_fm = marp.splitn(4, "---").collect::<Vec<_>>();
        // after_fm[0] = ""  (before opening ---)
        // after_fm[1] = "\nmarp: true\n..."  (front matter body)
        // after_fm[2] = "\n\n# First\n..."
        // after_fm[3] = "\n\n# Second\n..."  (slide two)
        assert!(
            after_fm.len() >= 4,
            "expected at least 4 `---` splits, got: {}",
            after_fm.len()
        );
    }

    #[test]
    fn test_rule_slide_separator() {
        let md = "Content A\n\n---\n\nContent B";
        let events = parser::parse(md);
        let marp = convert(events, &Config::default()).unwrap();
        // Must contain the `---\n\n` separator inside document body.
        let body_start = marp.find("---\n\n").expect("no front matter");
        // Skip the FM sentinel.
        let after_fm = &marp[body_start + 5..];
        assert!(
            after_fm.contains("---"),
            "rule separator not forwarded to marp output"
        );
    }

    #[test]
    fn test_inline_formatting() {
        let md = "**bold** and *italic* and `code`";
        let events = parser::parse(md);
        let marp = convert(events, &Config::default()).unwrap();
        assert!(marp.contains("**bold**"), "bold not serialized");
        assert!(marp.contains("*italic*"), "italic not serialized");
        assert!(marp.contains("`code`"), "inline code not serialized");
    }

    #[test]
    fn test_list_serialization() {
        let md = "- alpha\n- beta\n- gamma";
        let events = parser::parse(md);
        let marp = convert(events, &Config::default()).unwrap();
        assert!(marp.contains("- alpha"), "bullet list item missing");
        assert!(marp.contains("- beta"), "bullet list item missing");
    }

    #[test]
    fn test_table_generation() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let events = parser::parse(md);
        let marp = convert(events, &Config::default()).unwrap();
        assert!(marp.contains("| A |"), "table header not serialized");
        assert!(marp.contains("| 1 |"), "table row not serialized");
    }
}
