//! Mermaid diagram source → PNG rasterization.
//!
//! Pipeline:
//! 1. `marksmen_mermaid::parsing::parser::parse` → AST
//! 2. `ast_to_graph` → `DirectedGraph`
//! 3. `assign_ranks` → `minimize_crossings` → `assign_coordinates` → `SpacedGraph`
//! 4. `render_graph_to_svg` → SVG string (pure SVG, no foreignObject)
//! 5. `svg_bytes_to_png` → PNG bytes

use crate::svg::svg_bytes_to_png;
use marksmen_mermaid::{
    graph::directed_graph,
    layout::{coordinate_assign, crossing_reduction, rank_assignment},
    parsing::parser,
};

/// Parse a Mermaid diagram source string and render it to a PNG byte buffer.
///
/// # Returns
/// `Some((png_bytes, width_px, height_px))` on success, `None` on parse/render failure.
pub fn render_mmd_to_png(mmd_source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let ast = parser::parse(mmd_source).ok()?;
    let directed = directed_graph::ast_to_graph(ast);
    let mut ranked = rank_assignment::assign_ranks(&directed);
    crossing_reduction::minimize_crossings(&mut ranked);
    let spaced = coordinate_assign::assign_coordinates(&ranked);
    let svg = render_graph_to_svg(&spaced);
    svg_bytes_to_png(svg.as_bytes())
}

/// Render a `SpacedGraph` to a valid SVG string.
///
/// All elements use standard SVG primitives (`<rect>`, `<text>`, `<polyline>`,
/// `<polygon>`) with no `<foreignObject>`, ensuring full `usvg` compatibility.
pub fn render_graph_to_svg(
    graph: &marksmen_mermaid::layout::coordinate_assign::SpacedGraph,
) -> String {
    let padding = 20.0;
    let svg_width = graph.width + padding * 2.0;
    let svg_height = graph.height + padding * 2.0;

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
  <rect width="{w}" height="{h}" fill="white"/>"##,
        w = svg_width,
        h = svg_height,
    );

    // Subgraph backgrounds
    let mut ordered_subgraphs = graph.subgraphs.clone();
    ordered_subgraphs.sort_by(|l, r| l.depth.cmp(&r.depth).then_with(|| l.title.cmp(&r.title)));
    for sg in &ordered_subgraphs {
        let shade = (248.0 - (sg.depth as f64 * 8.0)).max(228.0) as i32;
        let fill = format!("#{:02x}{:02x}{:02x}", shade, shade, shade);
        let rx = (8.0 - sg.depth as f64).max(4.0);
        svg.push_str(&format!(
            r##"  <rect x="{}" y="{}" width="{}" height="{}" rx="{rx}" ry="{rx}" fill="{fill}" stroke="#aaaaaa" stroke-width="1"/>
"##,
            sg.x + padding,
            sg.y + padding,
            sg.width,
            sg.height,
        ));
        svg.push_str(&format!(
            r##"  <text x="{}" y="{}" font-family="Arial, sans-serif" font-size="11" font-weight="bold" fill="#666">{}</text>
"##,
            sg.x + padding + 10.0,
            sg.y + padding + 16.0,
            xml_escape(&sg.title),
        ));
    }

    // Edges
    for edge in &graph.edges {
        if edge.path.len() < 2 {
            continue;
        }
        use marksmen_mermaid::parsing::lexer::EdgeStyle;
        let stroke_width = match edge.style {
            EdgeStyle::ThickArrow => 2.5,
            _ => 2.0,
        };
        let dash = if edge.style == EdgeStyle::DottedArrow {
            r#" stroke-dasharray="4 4""#
        } else {
            ""
        };
        let stroke_path = if edge.style == EdgeStyle::SolidLine {
            edge.path.clone()
        } else {
            trim_path_end(&edge.path, 10.0)
        };
        let points = stroke_path
            .iter()
            .map(|(x, y)| format!("{},{}", x + padding, y + padding))
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            r##"  <polyline points="{points}" fill="none" stroke="#555" stroke-width="{stroke_width}"{dash} stroke-linejoin="round" stroke-linecap="round"/>
"##
        ));

        if edge.style != EdgeStyle::SolidLine {
            if let Some([tip, left, right]) = arrowhead_points(&edge.path, 10.0, 7.0) {
                svg.push_str(&format!(
                    r##"  <polygon points="{},{} {},{} {},{}" fill="#555" stroke="#555" stroke-width="0.6"/>
"##,
                    tip.0 + padding,
                    tip.1 + padding,
                    left.0 + padding,
                    left.1 + padding,
                    right.0 + padding,
                    right.1 + padding,
                ));
            }
        }

        if let Some(label) = &edge.label {
            let (lx, ly) = edge_label_anchor(&edge.path);
            let tx = lx + padding;
            let ty = ly + padding - 6.0;
            svg.push_str(&format!(
                r##"  <rect x="{}" y="{}" width="140" height="18" fill="white" opacity="0.9"/>
  <text x="{tx}" y="{ty}" text-anchor="middle" font-family="Arial, sans-serif" font-size="11" fill="#444">{}</text>
"##,
                tx - 70.0,
                ty - 12.0,
                xml_escape(label),
            ));
        }
    }

    // Nodes
    for (_id, node) in &graph.nodes {
        let rx = node.x + padding;
        let ry = node.y + padding;
        let fill = node.style.fill.as_deref().unwrap_or("#E8F4FD");
        let stroke = node.style.stroke.as_deref().unwrap_or("#2196F3");
        let sw = node.style.stroke_width.as_deref().unwrap_or("2");
        let text_fill = node.style.color.as_deref().unwrap_or("#333");
        let dash = node
            .style
            .stroke_dasharray
            .as_deref()
            .map(|v| format!(r#" stroke-dasharray="{v}""#))
            .unwrap_or_default();
        svg.push_str(&format!(
            r##"  <rect x="{rx}" y="{ry}" width="{}" height="{}" rx="6" ry="6" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"{dash}/>
"##,
            node.width, node.height,
        ));
        let text_x = rx + node.width / 2.0;
        let text_y = ry + node.height / 2.0 + 5.0;
        svg.push_str(&format!(
            r##"  <text x="{text_x}" y="{text_y}" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="{text_fill}">{}</text>
"##,
            xml_escape(&node.label),
        ));
    }

    svg.push_str("</svg>");
    svg
}

// ── helpers ────────────────────────────────────────────────────────────────

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn edge_label_anchor(path: &[(f64, f64)]) -> (f64, f64) {
    if path.len() < 2 {
        return (0.0, 0.0);
    }
    let mid = (path.len() - 1) / 2;
    let s = path[mid];
    let e = path[mid + 1];
    ((s.0 + e.0) / 2.0, (s.1 + e.1) / 2.0)
}

fn trim_path_end(path: &[(f64, f64)], distance: f64) -> Vec<(f64, f64)> {
    if path.len() < 2 {
        return path.to_vec();
    }
    let mut trimmed = path.to_vec();
    for idx in (1..trimmed.len()).rev() {
        let s = trimmed[idx - 1];
        let e = trimmed[idx];
        let dx = e.0 - s.0;
        let dy = e.1 - s.1;
        let mag = (dx * dx + dy * dy).sqrt();
        if mag < 0.001 {
            continue;
        }
        let applied = distance.min(mag * 0.6);
        trimmed[idx] = (e.0 - (dx / mag) * applied, e.1 - (dy / mag) * applied);
        return trimmed;
    }
    trimmed
}

fn arrowhead_points(path: &[(f64, f64)], len: f64, width: f64) -> Option<[(f64, f64); 3]> {
    for seg in path.windows(2).rev() {
        let (sx, sy) = seg[0];
        let (ex, ey) = seg[1];
        let dx = ex - sx;
        let dy = ey - sy;
        let mag = (dx * dx + dy * dy).sqrt();
        if mag < 0.001 {
            continue;
        }
        let ux = dx / mag;
        let uy = dy / mag;
        let px = -uy;
        let py = ux;
        let bx = ex - ux * len;
        let by = ey - uy * len;
        let hw = width / 2.0;
        return Some([
            (ex, ey),
            (bx + px * hw, by + py * hw),
            (bx - px * hw, by - py * hw),
        ]);
    }
    None
}
