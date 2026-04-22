//! XHTML writer: evaluates a marksmen AST event stream into a conformant
//! XHTML 1.1-polyglot document.
//!
//! ## XHTML-versus-HTML invariants enforced here
//!
//! | Property | HTML5 | This crate (XHTML) |
//! |---|---|---|
//! | XML declaration | absent | `<?xml version="1.0" encoding="UTF-8"?>` |
//! | `<html>` namespace | absent | `xmlns="http://www.w3.org/1999/xhtml"` |
//! | Void-element syntax | `<br>` | `<br />` |
//! | Attribute quoting | optional | mandatory double-quotes |
//! | Entity escaping | HTML rules | XML rules (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`) |
//! | MathML embedding | inline `<math>` | inline `<math xmlns="http://www.w3.org/1998/Math/MathML">` |
//!
//! The `scraper`-based reader (`marksmen-xhtml-read`) can reconstruct Markdown
//! from documents produced by this writer via the hidden roundtrip-metadata
//! convention (`<pre class="marksmen-roundtrip-meta">`).

use anyhow::Result;
use marksmen_core::Config;
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};

/// Converts a marksmen AST event slice into a conformant XHTML document string.
///
/// # Theorem (roundtrip identity)
/// For any Markdown source `M`, `parse_xhtml(convert(parse(M), cfg)) ≈ M`
/// up to whitespace normalization and Mermaid source extraction from the
/// hidden metadata block.
pub fn convert(events: Vec<Event<'_>>, config: &Config) -> Result<String> {
    let mut out = String::with_capacity(events.len() * 100);

    // XML declaration — required for XHTML served as `application/xhtml+xml`.
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    // DOCTYPE for XHTML 1.1 polyglot processors.
    out.push_str("<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\n");
    out.push_str("  \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n");
    // Root element with mandatory XHTML namespace.
    out.push_str("<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"en\">\n<head>\n");
    // All meta/link elements use self-closing form.
    out.push_str("  <meta charset=\"UTF-8\" />\n");
    out.push_str(
        "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n",
    );
    out.push_str(&format!(
        "  <title>{}</title>\n",
        escape_xml(config.title.as_str())
    ));
    out.push_str("  <style type=\"text/css\">\n");
    out.push_str("    body { font-family: 'Helvetica Neue', Arial, sans-serif; line-height: 1.6; max-width: 900px; margin: 0 auto; padding: 2rem; color: #333; }\n");
    out.push_str("    img { max-width: 100%; height: auto; }\n");
    out.push_str(
        "    table { border-collapse: collapse; width: 100%; margin: 1rem 0; }\n",
    );
    out.push_str(
        "    th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }\n",
    );
    out.push_str("    th { background-color: #f5f5f5; }\n");
    out.push_str("    pre { background: #f4f4f4; padding: 1rem; overflow-x: auto; }\n");
    out.push_str(
        "    blockquote { border-left: 4px solid #0366d6; margin: 0; padding-left: 1rem; color: #666; }\n",
    );
    out.push_str("  </style>\n");
    out.push_str("</head>\n<body>\n");

    if !config.title.is_empty() {
        out.push_str(&format!(
            "  <h1>{}</h1>\n",
            escape_xml(config.title.as_str())
        ));
    }
    if !config.author.is_empty() {
        out.push_str(&format!(
            "  <p><strong>{}</strong></p>\n",
            escape_xml(config.author.as_str())
        ));
    }

    let mut in_mermaid_block = false;
    let mut current_mermaid_source = String::new();

    let mut iter = events.into_iter();
    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::Paragraph) => out.push_str("<p>"),
            Event::End(TagEnd::Paragraph) => out.push_str("</p>\n"),
            Event::Start(Tag::Heading { level, .. }) => {
                out.push_str(&format!("<h{}>", level as usize))
            }
            Event::End(TagEnd::Heading(level)) => {
                out.push_str(&format!("</h{}>\n", level as usize))
            }
            Event::Start(Tag::BlockQuote(_)) => out.push_str("<blockquote>"),
            Event::End(TagEnd::BlockQuote(_)) => out.push_str("</blockquote>\n"),

            // Mermaid fenced block — accumulate source, emit inline SVG + hidden metadata.
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang)))
                if lang.as_ref() == "mermaid" =>
            {
                in_mermaid_block = true;
                current_mermaid_source.clear();
            }

            Event::Start(Tag::CodeBlock(_)) => out.push_str("<pre><code>"),
            Event::End(TagEnd::CodeBlock) => {
                if in_mermaid_block {
                    in_mermaid_block = false;
                    let ast =
                        marksmen_mermaid::parsing::parser::parse(&current_mermaid_source);
                    match ast {
                        Ok(a) => {
                            let directed_graph =
                                marksmen_mermaid::graph::directed_graph::ast_to_graph(a);
                            let mut ranked_graph =
                                marksmen_mermaid::layout::rank_assignment::assign_ranks(
                                    &directed_graph,
                                );
                            marksmen_mermaid::layout::crossing_reduction::minimize_crossings(
                                &mut ranked_graph,
                            );
                            let spaced_graph =
                                marksmen_mermaid::layout::coordinate_assign::assign_coordinates(
                                    &ranked_graph,
                                );

                            let svg_str = render_graph_to_svg(&spaced_graph);
                            out.push_str(
                                "<div class=\"mermaid-graph\" style=\"text-align: center; margin: 2rem 0;\">\n",
                            );
                            out.push_str(&svg_str);
                            out.push_str("\n</div>\n");
                            // Hidden metadata block preserves Mermaid source for roundtrip.
                            out.push_str(
                                "<pre class=\"marksmen-roundtrip-meta\" style=\"display:none\">```mermaid\n",
                            );
                            out.push_str(&escape_xml(&current_mermaid_source));
                            out.push_str("\n```</pre>\n");
                        }
                        Err(_) => {
                            out.push_str(
                                "<pre style=\"color: red;\"><code>[Mermaid Parsing Fault]\n",
                            );
                            out.push_str(&escape_xml(&current_mermaid_source));
                            out.push_str("</code></pre>\n");
                        }
                    }
                } else {
                    out.push_str("</code></pre>\n");
                }
            }

            Event::Start(Tag::List(Some(_))) => out.push_str("<ol>\n"),
            Event::Start(Tag::List(None)) => out.push_str("<ul>\n"),
            Event::End(TagEnd::List(is_ord)) => {
                if is_ord {
                    out.push_str("</ol>\n");
                } else {
                    out.push_str("</ul>\n");
                }
            }
            Event::Start(Tag::Item) => out.push_str("<li>"),
            Event::End(TagEnd::Item) => out.push_str("</li>\n"),

            Event::Start(Tag::Table(_)) => out.push_str("<table>\n"),
            Event::End(TagEnd::Table) => out.push_str("</table>\n"),
            Event::Start(Tag::TableHead) => out.push_str("  <thead>\n    <tr>\n"),
            Event::End(TagEnd::TableHead) => {
                out.push_str("    </tr>\n  </thead>\n  <tbody>\n")
            }
            Event::Start(Tag::TableRow) => out.push_str("    <tr>\n"),
            Event::End(TagEnd::TableRow) => out.push_str("    </tr>\n"),
            Event::Start(Tag::TableCell) => out.push_str("      <td>"),
            Event::End(TagEnd::TableCell) => out.push_str("</td>\n"),

            Event::Start(Tag::Emphasis) => out.push_str("<em>"),
            Event::End(TagEnd::Emphasis) => out.push_str("</em>"),
            Event::Start(Tag::Strong) => out.push_str("<strong>"),
            Event::End(TagEnd::Strong) => out.push_str("</strong>"),
            Event::Start(Tag::Strikethrough) => out.push_str("<del>"),
            Event::End(TagEnd::Strikethrough) => out.push_str("</del>"),

            Event::Start(Tag::Link { dest_url, .. }) => {
                out.push_str(&format!("<a href=\"{}\">", escape_xml(dest_url.as_ref())))
            }
            Event::End(TagEnd::Link) => out.push_str("</a>"),

            // img uses self-closing syntax — XHTML invariant.
            Event::Start(Tag::Image { dest_url, .. }) => {
                out.push_str(&format!(
                    "<img src=\"{}\" alt=\"",
                    escape_xml(dest_url.as_ref())
                ));
            }
            Event::End(TagEnd::Image) => out.push_str("\" />"),

            Event::Code(text) => out.push_str(&format!(
                "<code>{}</code>",
                escape_xml(text.as_ref())
            )),

            Event::Text(text) => {
                if in_mermaid_block {
                    current_mermaid_source.push_str(text.as_ref());
                } else {
                    out.push_str(&escape_xml(text.as_ref()));
                }
            }

            Event::Html(raw) => out.push_str(&raw),

            // XHTML: `<br />` not `<br>`.
            Event::SoftBreak | Event::HardBreak => out.push_str("<br />\n"),

            Event::InlineMath(math) => {
                match latex2mathml::latex_to_mathml(
                    math.as_ref(),
                    latex2mathml::DisplayStyle::Inline,
                ) {
                    Ok(mathml) => out.push_str(&qualify_mathml_ns(&mathml)),
                    Err(_) => out.push_str(&format!(
                        "<span class=\"math-inline\">{}</span>",
                        escape_xml(math.as_ref())
                    )),
                }
            }

            Event::DisplayMath(math) => {
                match latex2mathml::latex_to_mathml(
                    math.as_ref(),
                    latex2mathml::DisplayStyle::Block,
                ) {
                    Ok(mathml) => out.push_str(&qualify_mathml_ns(&mathml)),
                    Err(_) => out.push_str(&format!(
                        "<div class=\"math-display\">{}</div>\n",
                        escape_xml(math.as_ref())
                    )),
                }
            }

            // `<hr />` — page-break convention shared with marksmen-html.
            Event::Rule => out.push_str("<hr />\n"),

            _ => {}
        }
    }

    out.push_str("</body>\n</html>");
    Ok(out)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Escapes the five mandatory XML entities.
///
/// Invariant: `escape_xml(s)` produces a string safe for XML text content and
/// attribute values. Specifically, none of `& < > " '` appear unescaped.
fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Ensures the top-level `<math>` element carries the W3C MathML namespace
/// declaration when `latex2mathml` omits it.
///
/// `latex2mathml` emits `<math display="...">` without `xmlns`; an XHTML
/// document served as `application/xhtml+xml` requires the namespace to be
/// explicit so that MathML processors can recognize the element.
fn qualify_mathml_ns(mathml: &str) -> String {
    if mathml.starts_with("<math") && !mathml.contains("xmlns") {
        mathml.replacen(
            "<math",
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"",
            1,
        )
    } else {
        mathml.to_string()
    }
}

/// Renders a `SpacedGraph` into an SVG string.
///
/// The SVG is XML-compliant and can be embedded directly in XHTML.
fn render_graph_to_svg(
    graph: &marksmen_mermaid::layout::coordinate_assign::SpacedGraph,
) -> String {
    let padding = 20.0_f64;
    let svg_width = graph.width + padding * 2.0;
    let svg_height = graph.height + padding * 2.0;

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" style="max-width: 100%; height: auto;">
  <defs>
    <marker id="arrowhead" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto">
      <polygon points="0 0, 10 3.5, 0 7" fill="#333"/>
    </marker>
  </defs>
  <rect width="{w}" height="{h}" fill="white" rx="8" ry="8" stroke="#eeeeee" stroke-width="1"/>"##,
        w = svg_width,
        h = svg_height,
    );

    let mut ordered_subgraphs = graph.subgraphs.clone();
    ordered_subgraphs.sort_by(|l, r| l.depth.cmp(&r.depth).then_with(|| l.title.cmp(&r.title)));
    for subgraph in &ordered_subgraphs {
        let shade = (248.0 - (subgraph.depth as f64 * 8.0)).max(228.0) as i32;
        let fill = format!("#{:02x}{:02x}{:02x}", shade, shade, shade);
        svg.push_str(&format!(
            r##"  <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="{}" stroke="#aaaaaa" stroke-width="1"/>"##,
            subgraph.x + padding,
            subgraph.y + padding,
            subgraph.width,
            subgraph.height,
            (8.0 - subgraph.depth as f64).max(4.0),
            (8.0 - subgraph.depth as f64).max(4.0),
            fill,
        ));
        svg.push('\n');
        svg.push_str(&format!(
            r##"  <text x="{}" y="{}" font-family="Arial, sans-serif" font-size="11" font-weight="bold" fill="#666">{}</text>"##,
            subgraph.x + padding + 10.0,
            subgraph.y + padding + 16.0,
            escape_xml(&subgraph.title),
        ));
        svg.push('\n');
    }

    for edge in &graph.edges {
        if edge.path.len() >= 2 {
            let stroke_width = match edge.style {
                marksmen_mermaid::parsing::lexer::EdgeStyle::ThickArrow => 2.5,
                _ => 2.0,
            };
            let dash = if edge.style
                == marksmen_mermaid::parsing::lexer::EdgeStyle::DottedArrow
            {
                r#" stroke-dasharray="4 4""#
            } else {
                ""
            };
            let points = edge
                .path
                .iter()
                .map(|(x, y)| format!("{},{}", x + padding, y + padding))
                .collect::<Vec<_>>()
                .join(" ");
            svg.push_str(&format!(
                r##"  <polyline points="{}" fill="none" stroke="#555" stroke-width="{}"{} marker-end="url(#arrowhead)"/>"##,
                points, stroke_width, dash,
            ));
            svg.push('\n');

            if let Some(label) = &edge.label {
                if edge.path.len() >= 2 {
                    let mid = (edge.path.len() - 1) / 2;
                    let start = edge.path[mid];
                    let end = edge.path[mid + 1];
                    let lx = (start.0 + end.0) / 2.0 + padding;
                    let ly = (start.1 + end.1) / 2.0 + padding - 6.0;
                    svg.push_str(&format!(
                        r##"  <rect x="{}" y="{}" width="140" height="18" fill="white" opacity="0.9"/>"##,
                        lx - 70.0,
                        ly - 12.0,
                    ));
                    svg.push('\n');
                    svg.push_str(&format!(
                        r##"  <text x="{}" y="{}" text-anchor="middle" font-family="Arial, sans-serif" font-size="11" fill="#444">{}</text>"##,
                        lx,
                        ly,
                        escape_xml(label),
                    ));
                    svg.push('\n');
                }
            }
        }
    }

    for (_id, node) in &graph.nodes {
        let rx = node.x + padding;
        let ry = node.y + padding;
        let fill = node.style.fill.as_deref().unwrap_or("#E8F4FD");
        let stroke = node.style.stroke.as_deref().unwrap_or("#2196F3");
        let stroke_width = node.style.stroke_width.as_deref().unwrap_or("2");
        let text_fill = node.style.color.as_deref().unwrap_or("#333");
        let dash = node
            .style
            .stroke_dasharray
            .as_deref()
            .map(|v| format!(r#" stroke-dasharray="{}""#, v))
            .unwrap_or_default();
        svg.push_str(&format!(
            r##"  <rect x="{}" y="{}" width="{}" height="{}" rx="6" ry="6" fill="{}" stroke="{}" stroke-width="{}"{} />"##,
            rx, ry, node.width, node.height, fill, stroke, stroke_width, dash,
        ));
        svg.push('\n');
        let text_x = rx + node.width / 2.0;
        let text_y = ry + node.height / 2.0 + 5.0;
        svg.push_str(&format!(
            r##"  <text x="{}" y="{}" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="{}">{}</text>"##,
            text_x,
            text_y,
            text_fill,
            escape_xml(&node.label),
        ));
        svg.push('\n');
    }

    svg.push_str("</svg>");
    svg
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{convert, escape_xml, qualify_mathml_ns};
    use marksmen_core::Config;
    use pulldown_cmark::{Event, Tag, TagEnd};

    #[test]
    fn escape_xml_covers_all_five_entities() {
        let input = "a & b < c > d \" e ' f";
        let escaped = escape_xml(input);
        assert_eq!(escaped, "a &amp; b &lt; c &gt; d &quot; e &apos; f");
    }

    #[test]
    fn qualify_mathml_ns_adds_namespace_when_absent() {
        let bare = r#"<math display="block"><mi>x</mi></math>"#;
        let qualified = qualify_mathml_ns(bare);
        assert!(qualified.contains("xmlns=\"http://www.w3.org/1998/Math/MathML\""));
    }

    #[test]
    fn qualify_mathml_ns_is_idempotent_when_namespace_present() {
        let already = r#"<math xmlns="http://www.w3.org/1998/Math/MathML"><mi>x</mi></math>"#;
        let qualified = qualify_mathml_ns(already);
        // Namespace must not be duplicated.
        assert_eq!(
            qualified.matches("xmlns=").count(),
            1,
            "namespace must appear exactly once"
        );
    }

    #[test]
    fn convert_produces_xml_declaration_and_xhtml_namespace() {
        let events: Vec<Event<'static>> = vec![
            Event::Start(Tag::Paragraph),
            Event::Text("Hello".into()),
            Event::End(TagEnd::Paragraph),
        ];
        let config = Config::default();
        let xhtml = convert(events, &config).unwrap();
        assert!(
            xhtml.starts_with("<?xml version=\"1.0\""),
            "must begin with XML declaration"
        );
        assert!(
            xhtml.contains("xmlns=\"http://www.w3.org/1999/xhtml\""),
            "must carry XHTML namespace"
        );
        assert!(
            xhtml.contains("<p>Hello</p>"),
            "paragraph content must be present"
        );
    }

    #[test]
    fn convert_void_elements_are_self_closing() {
        let events: Vec<Event<'static>> = vec![Event::SoftBreak];
        let config = Config::default();
        let xhtml = convert(events, &config).unwrap();
        assert!(
            xhtml.contains("<br />"),
            "br must use self-closing form in XHTML"
        );
    }

    #[test]
    fn convert_escapes_text_content() {
        let events: Vec<Event<'static>> = vec![
            Event::Start(Tag::Paragraph),
            Event::Text("<script>alert('xss')</script>".into()),
            Event::End(TagEnd::Paragraph),
        ];
        let config = Config::default();
        let xhtml = convert(events, &config).unwrap();
        assert!(
            !xhtml.contains("<script>"),
            "raw script tags must be escaped"
        );
        assert!(xhtml.contains("&lt;script&gt;"));
    }
}
