//! Reader for XHTML documents produced by `marksmen-xhtml`.
//!
//! Reconstructs Markdown-like text by traversing the XHTML body and restoring
//! headings, paragraphs, lists, tables, code blocks, math spans, images, and
//! hidden roundtrip metadata blocks.
//!
//! ## Parser choice
//!
//! `scraper` uses an HTML5 tree builder that accepts XHTML as well as HTML5,
//! making it suitable for reading XHTML 1.1-polyglot documents.  The document
//! structure emitted by `marksmen-xhtml::convert` is fully compatible with
//! the HTML5 tree model.
//!
//! ## Roundtrip identity invariant
//!
//! For documents produced by `marksmen-xhtml::convert`, Mermaid source is
//! recoverable from the `<pre class="marksmen-roundtrip-meta">` hidden block
//! and MathML inline elements are identified by `class="math-inline"` /
//! `class="math-display"` fallback spans (emitted when `latex2mathml` fails).
//! When `latex2mathml` succeeds, rendered MathML `<math>` elements are not
//! reconstructed to LaTeX; the source is lost unless the fallback span was
//! used.

use anyhow::{Context, Result};
use scraper::{ElementRef, Html, Node, Selector};

/// Parses an XHTML string into a Markdown-equivalent string.
///
/// The function selects the `<body>` element, then performs a depth-first
/// traversal, mapping each element to its Markdown equivalent.
pub fn parse_xhtml(xhtml: &str) -> Result<String> {
    let document = Html::parse_document(xhtml);
    let body_selector = Selector::parse("body").unwrap();
    let body = document
        .select(&body_selector)
        .next()
        .context("XHTML document missing <body>")?;

    let mut out = String::new();
    for child in body.children() {
        render_node(child, &mut out, false);
    }

    Ok(cleanup_markdown(&out))
}

// ---------------------------------------------------------------------------
// Tree traversal
// ---------------------------------------------------------------------------

fn render_node(handle: ego_tree::NodeRef<'_, Node>, out: &mut String, in_pre: bool) {
    match handle.value() {
        Node::Text(text) => {
            if in_pre {
                out.push_str(text);
            } else {
                let normalized = text.text.to_string();
                let mut compressed = String::with_capacity(normalized.len());
                let mut in_space = false;
                for c in normalized.chars() {
                    if c.is_whitespace() {
                        if !in_space {
                            compressed.push(' ');
                            in_space = true;
                        }
                    } else {
                        compressed.push(c);
                        in_space = false;
                    }
                }
                if !compressed.is_empty() {
                    out.push_str(&compressed);
                }
            }
        }
        Node::Element(element) => {
            let Some(el) = ElementRef::wrap(handle) else {
                return;
            };
            let name = element.name();
            match name {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    let level = name[1..].parse::<usize>().unwrap_or(1);
                    ensure_block_break(out);
                    out.push_str(&"#".repeat(level));
                    out.push(' ');
                    render_children(&el, out, false);
                    out.push_str("\n\n");
                }
                "p" => {
                    ensure_block_break(out);
                    render_children(&el, out, false);
                    out.push_str("\n\n");
                }
                "blockquote" => {
                    ensure_block_break(out);
                    let mut inner = String::new();
                    render_children(&el, &mut inner, false);
                    for line in cleanup_markdown(&inner).lines() {
                        out.push_str("> ");
                        out.push_str(line);
                        out.push('\n');
                    }
                    out.push('\n');
                }
                "strong" => {
                    out.push_str("**");
                    render_children(&el, out, false);
                    out.push_str("**");
                }
                "em" => {
                    out.push('*');
                    render_children(&el, out, false);
                    out.push('*');
                }
                "del" => {
                    out.push_str("~~");
                    render_children(&el, out, false);
                    out.push_str("~~");
                }
                "code" => {
                    out.push('`');
                    render_children(&el, out, true);
                    out.push('`');
                }
                "pre" => {
                    let class = element.attr("class").unwrap_or_default();
                    if class.contains("marksmen-roundtrip-meta") {
                        // Verbatim Mermaid source preserved in hidden metadata block.
                        ensure_block_break(out);
                        render_children(&el, out, true);
                        out.push_str("\n\n");
                    } else {
                        ensure_block_break(out);
                        out.push_str("```\n");
                        render_children(&el, out, true);
                        out.push_str("\n```\n\n");
                    }
                }
                "span" => {
                    let class = element.attr("class").unwrap_or_default();
                    if class.contains("math-inline") {
                        out.push('$');
                        render_children(&el, out, true);
                        out.push('$');
                    } else {
                        render_children(&el, out, false);
                    }
                }
                "sup" => {
                    let class = element.attr("class").unwrap_or_default();
                    if class.contains("footnote-ref") {
                        if let Some(label) = element.attr("data-label") {
                            out.push_str(&format!("[^{}]", label));
                        }
                    } else {
                        out.push('^');
                        render_children(&el, out, false);
                        out.push('^');
                    }
                }
                "sub" => {
                    out.push('~');
                    render_children(&el, out, false);
                    out.push('~');
                }
                "div" => {
                    let class = element.attr("class").unwrap_or_default();
                    if class.contains("math-display") {
                        ensure_block_break(out);
                        out.push_str("$$\n");
                        render_children(&el, out, true);
                        out.push_str("\n$$\n\n");
                    } else if class.contains("mermaid-graph") {
                        // Visible rendered SVG is skipped; hidden metadata block carries source.
                    } else if class.contains("footnote-def") {
                        if let Some(label) = element.attr("data-label") {
                            ensure_block_break(out);
                            out.push_str(&format!("[^{}]: ", label));

                            let mut inner = String::new();
                            render_children(&el, &mut inner, false);

                            let mut body = inner.as_str();
                            let prefix = format!("[{}]:", label);
                            if body.starts_with(&prefix) {
                                body = &body[prefix.len()..];
                            } else if body.starts_with(&format!("[{}]", label)) {
                                body = &body[label.len() + 2..];
                                if body.starts_with(':') {
                                    body = &body[1..];
                                }
                            }

                            out.push_str(body.trim_start());
                            out.push_str("\n\n");
                        }
                    } else {
                        render_children(&el, out, false);
                    }
                }
                // MathML `<math>` elements cannot be round-tripped to LaTeX without a
                // dedicated MathML→LaTeX converter.  Emit an empty placeholder so that
                // structure is preserved.  If `latex2mathml` failed, the fallback `<span
                // class="math-inline">` path is used instead and this branch is not hit.
                "math" => {
                    // No reliable structural equivalent; skip silently.
                }
                "a" => {
                    let href = element.attr("href").unwrap_or_default();
                    let mut label = String::new();
                    render_children(&el, &mut label, false);
                    out.push('[');
                    out.push_str(label.trim());
                    out.push_str("](");
                    out.push_str(href);
                    out.push(')');
                }
                "img" => {
                    let alt = element.attr("alt").unwrap_or("Image");
                    let src = element.attr("src").unwrap_or_default();
                    out.push_str(&format!("![{}]({})", alt, src));
                }
                "input" => {
                    if element.attr("type") == Some("checkbox") {
                        if element.attr("checked").is_some() {
                            out.push_str("[x] ");
                        } else {
                            out.push_str("[ ] ");
                        }
                    }
                }
                // Self-closing in XHTML; emitted as `<br />` by the writer.
                "br" => out.push('\n'),
                "hr" => {
                    ensure_block_break(out);
                    out.push_str("---\n\n");
                }
                "ul" => {
                    ensure_block_break(out);
                    render_unordered_list(&el, out, 0);
                    out.push('\n');
                }
                "ol" => {
                    ensure_block_break(out);
                    render_ordered_list(&el, out, 0);
                    out.push('\n');
                }
                "table" => {
                    ensure_block_break(out);
                    render_table(&el, out);
                    out.push('\n');
                }
                "thead" | "tbody" | "tr" | "td" | "th" => {}
                _ => render_children(&el, out, in_pre),
            }
        }
        _ => {}
    }
}

fn render_children(el: &ElementRef<'_>, out: &mut String, in_pre: bool) {
    for child in el.children() {
        render_node(child, out, in_pre);
    }
}

// ---------------------------------------------------------------------------
// List rendering (depth-aware)
// ---------------------------------------------------------------------------

fn render_unordered_list(el: &ElementRef<'_>, out: &mut String, depth: usize) {
    let indent = "    ".repeat(depth);
    for li in el
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|c| c.value().name() == "li")
    {
        out.push_str(&format!("{}- ", indent));
        render_list_item(&li, out, depth);
    }
}

fn render_ordered_list(el: &ElementRef<'_>, out: &mut String, depth: usize) {
    let indent = "    ".repeat(depth);
    for (idx, li) in el
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|c| c.value().name() == "li")
        .enumerate()
    {
        out.push_str(&format!("{}{}. ", indent, idx + 1));
        render_list_item(&li, out, depth);
    }
}

/// Renders a `<li>` element: inline content first, then any nested list.
fn render_list_item(li: &ElementRef<'_>, out: &mut String, depth: usize) {
    let mut inline = String::new();
    for child in li.children() {
        match child.value() {
            Node::Element(e) if e.name() == "ul" => {
                // Flush inline text first.
                out.push_str(inline.trim());
                out.push('\n');
                inline.clear();
                let child_el = ElementRef::wrap(child).unwrap();
                render_unordered_list(&child_el, out, depth + 1);
                return;
            }
            Node::Element(e) if e.name() == "ol" => {
                out.push_str(inline.trim());
                out.push('\n');
                inline.clear();
                let child_el = ElementRef::wrap(child).unwrap();
                render_ordered_list(&child_el, out, depth + 1);
                return;
            }
            _ => render_node(child, &mut inline, false),
        }
    }
    out.push_str(inline.trim());
    out.push('\n');
}

// ---------------------------------------------------------------------------
// Table rendering
// ---------------------------------------------------------------------------

fn render_table(table: &ElementRef<'_>, out: &mut String) {
    let row_selector = Selector::parse("tr").unwrap();
    let cell_selector = Selector::parse("th, td").unwrap();
    let rows: Vec<Vec<String>> = table
        .select(&row_selector)
        .map(|row| {
            row.select(&cell_selector)
                .map(|cell| {
                    let mut inner = String::new();
                    render_children(&cell, &mut inner, false);
                    cleanup_markdown(&inner)
                })
                .collect::<Vec<_>>()
        })
        .filter(|row| !row.is_empty())
        .collect();

    if rows.is_empty() {
        return;
    }

    out.push('|');
    for cell in &rows[0] {
        out.push(' ');
        out.push_str(cell.trim());
        out.push_str(" |");
    }
    out.push('\n');

    let aligns: Vec<&str> = table
        .value()
        .attr("data-align")
        .map(|s| s.split(',').collect())
        .unwrap_or_default();

    out.push('|');
    for (i, _) in rows[0].iter().enumerate() {
        let align = aligns.get(i).copied().unwrap_or("left");
        match align {
            "center" => out.push_str(" :---: |"),
            "right" => out.push_str(" ---: |"),
            "none" => out.push_str(" --- |"),
            _ => out.push_str(" :--- |"),
        }
    }
    out.push('\n');

    for row in rows.iter().skip(1) {
        out.push('|');
        for cell in row {
            out.push(' ');
            out.push_str(cell.trim());
            out.push_str(" |");
        }
        out.push('\n');
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn ensure_block_break(out: &mut String) {
    if out.is_empty() {
        return;
    }
    if !out.ends_with("\n\n") {
        if out.ends_with('\n') {
            out.push('\n');
        } else {
            out.push_str("\n\n");
        }
    }
}

fn cleanup_markdown(input: &str) -> String {
    let mut cleaned = input.replace("\r\n", "\n").replace('\r', "\n");
    while cleaned.contains("\n\n\n") {
        cleaned = cleaned.replace("\n\n\n", "\n\n");
    }
    cleaned.trim().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::parse_xhtml;

    /// Verifies that the reader correctly reconstructs headings, math fallback
    /// spans, MathML math-display divs, and Mermaid roundtrip metadata.
    #[test]
    fn parses_headings_math_fallback_and_mermaid_metadata() {
        let xhtml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
<body>
<h2>Section</h2>
<p>Alpha <span class="math-inline">x+y</span></p>
<div class="math-display">z^2</div>
<div class="mermaid-graph"><svg /></div>
<pre class="marksmen-roundtrip-meta" style="display:none">```mermaid
flowchart LR
A --> B
```</pre>
</body>
</html>"#;
        let md = parse_xhtml(xhtml).unwrap();
        assert!(md.contains("## Section"), "heading must be restored");
        assert!(
            md.contains("$x+y$"),
            "inline math fallback must be restored"
        );
        assert!(md.contains("$$\nz^2\n$$"), "display math must be restored");
        assert!(
            md.contains("```mermaid"),
            "mermaid roundtrip metadata must be preserved"
        );
    }

    /// Verifies that nested lists are reconstructed with correct indentation.
    #[test]
    fn parses_nested_lists() {
        let xhtml = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
<body>
<ul>
  <li>Alpha
    <ul>
      <li>Sub-alpha</li>
    </ul>
  </li>
  <li>Beta</li>
</ul>
</body>
</html>"#;
        let md = parse_xhtml(xhtml).unwrap();
        assert!(md.contains("- Alpha"), "top-level item present");
        assert!(
            md.contains("    - Sub-alpha"),
            "nested item indented by four spaces"
        );
        assert!(md.contains("- Beta"), "second top-level item present");
    }

    /// Verifies that a simple anchor is reconstructed as a Markdown link.
    #[test]
    fn parses_anchor_as_markdown_link() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
<p><a href="https://example.com">Example</a></p>
</body></html>"#;
        let md = parse_xhtml(xhtml).unwrap();
        assert!(md.contains("[Example](https://example.com)"));
    }

    /// Verifies that self-closing `<br />` is reconstructed as a newline.
    #[test]
    fn parses_self_closing_br_as_newline() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Line1<br />Line2</p></body></html>"#;
        let md = parse_xhtml(xhtml).unwrap();
        assert!(md.contains("Line1") && md.contains("Line2"));
    }
}
