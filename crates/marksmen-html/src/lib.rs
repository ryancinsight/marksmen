//! HTML target builder evaluating marksmen AST flows into strict, zero-dependency HTML5 arrays.

use anyhow::Result;
use marksmen_core::Config;
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};

pub fn convert(events: Vec<Event<'_>>, config: &Config) -> Result<String> {
    let mut out = String::with_capacity(events.len() * 100);

    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("  <meta charset=\"UTF-8\">\n");
    out.push_str("  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    out.push_str(&format!(
        "  <title>{}</title>\n",
        marksmen_xml::escape(&config.title)
    ));
    out.push_str("  <style>\n");
    out.push_str("    body { font-family: 'Helvetica Neue', Arial, sans-serif; line-height: 1.6; max-width: 900px; margin: 0 auto; padding: 2rem; color: #333; }\n");
    out.push_str("    img { max-width: 100%; height: auto; }\n");
    out.push_str("    table { border-collapse: collapse; width: 100%; margin: 1rem 0; }\n");
    out.push_str("    th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }\n");
    out.push_str("    th { background-color: #f5f5f5; }\n");
    out.push_str("    pre { background: #f4f4f4; padding: 1rem; overflow-x: auto; }\n");
    out.push_str("    blockquote { border-left: 4px solid #0366d6; margin: 0; padding-left: 1rem; color: #666; }\n");
    out.push_str("    .footnote-ref { color: #0f6cbd; text-decoration: none; cursor: pointer; font-weight: 500; }\n");
    out.push_str("    .footnote-def { border-top: 1px solid #ddd; margin-top: 2rem; padding-top: 1rem; font-size: 0.9em; }\n");
    out.push_str("  </style>\n");
    out.push_str("</head>\n<body>\n");

    if !config.title.is_empty() {
        out.push_str(&format!(
            "  <h1>{}</h1>\n",
            marksmen_xml::escape(&config.title)
        ));
    }
    if !config.author.is_empty() {
        out.push_str(&format!(
            "  <p><strong>{}</strong></p>\n",
            marksmen_xml::escape(&config.author)
        ));
    }

    let mut in_mermaid_block = false;
    let mut current_mermaid_source = String::new();

    let iter = events.into_iter();
    for event in iter {
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
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang)))
                if lang.as_ref().starts_with("mermaid") =>
            {
                in_mermaid_block = true;
                current_mermaid_source.clear();
            }
            Event::Start(Tag::CodeBlock(_)) => out.push_str("<pre><code>"),
            Event::End(TagEnd::CodeBlock) => {
                if in_mermaid_block {
                    in_mermaid_block = false;
                    let ast = marksmen_mermaid::parsing::parser::parse(&current_mermaid_source);
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

                            // Native inline SVG requires evaluating the graph into our SVG string block.
                            let svg_str = render_graph_to_svg(&spaced_graph);
                            out.push_str("<div class=\"mermaid-graph\" style=\"text-align: center; margin: 2rem 0;\">\n");
                            out.push_str(&svg_str);
                            out.push_str("\n</div>\n");
                            out.push_str("<pre class=\"marksmen-roundtrip-meta\" style=\"display:none\">```mermaid\n");
                            out.push_str(&marksmen_xml::escape(&current_mermaid_source));
                            out.push_str("\n```</pre>\n");
                        }
                        Err(_) => {
                            out.push_str(
                                "<pre style=\"color: red;\"><code>[Mermaid Parsing Fault]\n",
                            );
                            out.push_str(&current_mermaid_source);
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
            Event::End(TagEnd::TableHead) => out.push_str("    </tr>\n  </thead>\n  <tbody>\n"),
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
                out.push_str(&format!("<a href=\"{}\">", dest_url))
            }
            Event::End(TagEnd::Link) => out.push_str("</a>"),
            Event::Start(Tag::Image { dest_url, .. }) => {
                out.push_str(&format!("<img src=\"{}\" alt=\"", dest_url));
            }
            Event::End(TagEnd::Image) => out.push_str("\" />"),
            Event::Code(text) => out.push_str(&format!(
                "<code>{}</code>",
                marksmen_xml::escape(text.as_ref())
            )),
            Event::Text(text) => {
                if in_mermaid_block {
                    current_mermaid_source.push_str(text.as_ref());
                } else {
                    out.push_str(&marksmen_xml::escape(text.as_ref()));
                }
            }
            Event::Html(html) | Event::InlineHtml(html) => out.push_str(&html),
            Event::FootnoteReference(label) => {
                out.push_str(&format!(
                    "<sup class=\"footnote-ref\" data-label=\"{}\">[{}]</sup>",
                    marksmen_xml::escape(label.as_ref()),
                    marksmen_xml::escape(label.as_ref())
                ));
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                out.push_str(&format!(
                    "<div class=\"footnote-def\" data-label=\"{}\"><b>[{}]</b>: ",
                    marksmen_xml::escape(label.as_ref()),
                    marksmen_xml::escape(label.as_ref())
                ));
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                out.push_str("</div>\n");
            }
            Event::SoftBreak | Event::HardBreak => out.push_str("<br />"),
            Event::InlineMath(math) => {
                match latex2mathml::latex_to_mathml(
                    math.as_ref(),
                    latex2mathml::DisplayStyle::Inline,
                ) {
                    Ok(mathml) => out.push_str(&mathml),
                    Err(_) => out.push_str(&format!(
                        "<span class=\"math-inline\">{}</span>",
                        marksmen_xml::escape(math.as_ref())
                    )),
                }
            }
            Event::DisplayMath(math) => {
                match latex2mathml::latex_to_mathml(
                    math.as_ref(),
                    latex2mathml::DisplayStyle::Block,
                ) {
                    Ok(mathml) => out.push_str(&mathml),
                    Err(_) => out.push_str(&format!(
                        "<div class=\"math-display\">{}</div>\n",
                        marksmen_xml::escape(math.as_ref())
                    )),
                }
            }
            Event::Rule => out.push_str("<div style=\"page-break-after: always;\"></div>\n"),
            _ => {}
        }
    }

    out.push_str("</body>\n</html>");
    Ok(out)
}

/// Helper isolated from marksmen-docx bounds to bypass the rasterizer
fn render_graph_to_svg(graph: &marksmen_mermaid::layout::coordinate_assign::SpacedGraph) -> String {
    let padding = 20.0;
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
        h = svg_height
    );

    let mut ordered_subgraphs = graph.subgraphs.clone();
    ordered_subgraphs.sort_by(|left, right| {
        left.depth
            .cmp(&right.depth)
            .then_with(|| left.title.cmp(&right.title))
    });
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
            fill
        ));
        svg.push('\n');
        svg.push_str(&format!(
            r##"  <text x="{}" y="{}" font-family="Arial, sans-serif" font-size="11" font-weight="bold" fill="#666">{}</text>"##,
            subgraph.x + padding + 10.0,
            subgraph.y + padding + 16.0,
            subgraph.title.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;")
        ));
        svg.push('\n');
    }

    // Draw edges
    for edge in &graph.edges {
        if edge.path.len() >= 2 {
            let stroke_width = match edge.style {
                marksmen_mermaid::parsing::lexer::EdgeStyle::ThickArrow => 2.5,
                _ => 2.0,
            };
            let dash = if edge.style == marksmen_mermaid::parsing::lexer::EdgeStyle::DottedArrow {
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
                points,
                stroke_width,
                dash,
            ));
            svg.push('\n');

            if let Some(label) = &edge.label
                && edge.path.len() >= 2 {
                    let mid_segment = (edge.path.len() - 1) / 2;
                    let start = edge.path[mid_segment];
                    let end = edge.path[mid_segment + 1];
                    let label_x = (start.0 + end.0) / 2.0;
                    let label_y = (start.1 + end.1) / 2.0;
                    let text_x = label_x + padding;
                    let text_y = label_y + padding - 6.0;
                    svg.push_str(&format!(
                        r##"  <rect x="{}" y="{}" width="140" height="18" fill="white" opacity="0.9"/>"##,
                        text_x - 70.0,
                        text_y - 12.0
                    ));
                    svg.push('\n');
                    svg.push_str(&format!(
                        r##"  <text x="{}" y="{}" text-anchor="middle" font-family="Arial, sans-serif" font-size="11" fill="#444">{}</text>"##,
                        text_x,
                        text_y,
                        label.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;")
                    ));
                    svg.push('\n');
                }
        }
    }

    // Draw nodes
    for node in graph.nodes.values() {
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
            rx, ry, node.width, node.height, fill, stroke, stroke_width, dash
        ));
        svg.push('\n');
        let text_x = rx + node.width / 2.0;
        let text_y = ry + node.height / 2.0 + 5.0;
        svg.push_str(&format!(
            r##"  <text x="{}" y="{}" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="{}">{}</text>"##,
            text_x, text_y, text_fill, node.label.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;")
        ));
        svg.push('\n');
    }

    svg.push_str("</svg>");
    svg
}
