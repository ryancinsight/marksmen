//! Typst Rendering Backend (Phase 4)
//!
//! Emits pure Typst primitives based on mathematically assigned Cartesian coordinates.

use crate::layout::coordinate_assign::SpacedGraph;
use rustc_hash::FxHashMap;

/// Transforms the deterministic Cartesian representation into a deterministic Typst block.
pub fn render_to_typst(graph: &SpacedGraph) -> String {
    let mut out = String::new();

    // Establish bounding block for the diagram
    // By wrapping in an `#align(center)[#box(...)]` we ensure Typst calculates its footprint.
    // The `clip: false` allows elements to spill if slightly miscalculated without cropping.
    // To solve page-cutoff, we wrap the whole thing in a `scale` block to fit width if necessary.
    out.push_str(&format!(
        "#align(center)[\n  #scale(x: 100%, y: 100%, reflow: true)[\n    #box(\n      width: {}pt,\n      height: {}pt,\n      clip: false\n    )[\n",
        graph.width + 100.0, // generous horizontal padding
        graph.height + 150.0 // exceedingly generous vertical padding to prevent bottom text cutoff
    ));

    let mut ordered_subgraphs = graph.subgraphs.clone();
    ordered_subgraphs.sort_by(|left, right| left.depth.cmp(&right.depth).then_with(|| left.title.cmp(&right.title)));
    for subgraph in &ordered_subgraphs {
        let shade = (248.0 - (subgraph.depth as f64 * 8.0)).max(228.0) as i32;
        let fill = format!("#{:02x}{:02x}{:02x}", shade, shade, shade);
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: {}pt, height: {}pt, radius: {}pt, fill: rgb(\"{}\"), stroke: 1pt + rgb(\"#aaaaaa\"))[#place(dx: 10pt, dy: 6pt)[#text(size: 9pt, weight: \"bold\", fill: rgb(\"#666666\"))[{}]]]]\n",
            subgraph.x + 20.0,
            subgraph.y + 20.0,
            subgraph.width,
            subgraph.height,
            (8.0 - subgraph.depth as f64).max(4.0),
            fill,
            escape_typst_text(&subgraph.title)
        ));
    }

    // Render edges first so they sit behind nodes
    for edge in &graph.edges {
        if edge.path.len() >= 2 {
            let stroke_path = if has_arrowhead(edge.style.clone()) {
                trim_path_end(&edge.path, 10.0)
            } else {
                edge.path.clone()
            };
            let path_points = stroke_path.iter()
                .map(|(x, y)| format!("({}pt, {}pt)", x + 20.0, y + 20.0))
                .collect::<Vec<_>>()
                .join(", ");

            out.push_str(&format!(
                "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: {}, closed: false, {})]\n",
                edge_stroke(edge),
                path_points
            ));

            if has_arrowhead(edge.style.clone()) {
                if let Some([tip, left, right]) = arrowhead_points(&edge.path, 10.0, 7.0) {
                    out.push_str(&format!(
                        "  #place(dx: 0pt, dy: 0pt)[#path(fill: rgb(\"#555555\"), stroke: 0.6pt + rgb(\"#555555\"), closed: true, ({}pt, {}pt), ({}pt, {}pt), ({}pt, {}pt))]\n",
                        tip.0 + 20.0,
                        tip.1 + 20.0,
                        left.0 + 20.0,
                        left.1 + 20.0,
                        right.0 + 20.0,
                        right.1 + 20.0,
                    ));
                } else {
                    let (end_x, end_y) = edge.path[edge.path.len() - 1];
                    out.push_str(&format!(
                        "  #place(dx: {}pt, dy: {}pt)[#text(size: 14pt)[▼]]\n",
                        end_x + 14.0,
                        end_y + 10.0
                    ));
                }
            }

            if let Some(label) = &edge.label {
                let (label_x, label_y) = edge_label_anchor(&edge.path);
                out.push_str(&format!(
                    "  #place(dx: {}pt, dy: {}pt)[#box(fill: white, inset: 3pt)[#text(size: 10pt, fill: rgb(\"#444444\"))[{}]]]\n",
                    label_x + 20.0,
                    label_y + 12.0,
                    escape_typst_text(label),
                ));
            }
        }
    }

    // Render nodes
    for (_, geom) in &graph.nodes {
        // Construct visual shape based on AST
        let shape_str = match geom.shape {
            Some(crate::parsing::lexer::NodeShape::Round) => "radius: 10pt",
            Some(crate::parsing::lexer::NodeShape::Circle) => "radius: 50%",
            Some(crate::parsing::lexer::NodeShape::Rhombus) => "radius: 0pt", // TODO: Polygon path for pure rhombus
            _ => "radius: 0pt", // Default square
        };
        let fill = typst_color(geom.style.fill.as_deref()).unwrap_or_else(|| "white".to_string());
        let stroke = typst_stroke(
            geom.style.stroke.as_deref().unwrap_or("#333333"),
            geom.style.stroke_width.as_deref().unwrap_or("1.5"),
        );
        let text_fill = typst_color(geom.style.color.as_deref()).unwrap_or_else(|| "rgb(\"#222222\")".to_string());

        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: {}pt, height: {}pt, {}, fill: {}, stroke: {})[#align(center + horizon)[#text(fill: {})[{}]]]]\n",
            geom.x + 20.0, geom.y + 20.0,
            geom.width, geom.height,
            shape_str,
            fill,
            stroke,
            text_fill,
            escape_typst_text(&geom.label)
        ));
    }

    out.push_str("    ]\n  ]\n]\n");
    out
}

/// Convenience entry point to parse, route, and render a raw Mermaid string directly to Typst
pub fn mermaid_to_typst(input: &str) -> anyhow::Result<String> {
    let trimmed = input.trim_start();
    if trimmed.starts_with("sequenceDiagram") {
        return render_sequence_to_typst(input);
    }
    if trimmed.starts_with("classDiagram") {
        return render_class_diagram_to_typst(input);
    }
    if trimmed.starts_with("erDiagram") {
        return render_er_diagram_to_typst(input);
    }
    if trimmed.starts_with("gantt") {
        return render_gantt_to_typst(input);
    }
    if trimmed.starts_with("pie") {
        return render_pie_to_typst(input);
    }
    if trimmed.starts_with("mindmap") {
        return render_mindmap_to_typst(input);
    }
    if trimmed.starts_with("timeline") {
        return render_timeline_to_typst(input);
    }
    if trimmed.starts_with("journey") {
        return render_journey_to_typst(input);
    }
    if trimmed.starts_with("stateDiagram") {
        return render_state_diagram_to_typst(input);
    }

    let ast = crate::parsing::parser::parse(input)
        .map_err(|e| anyhow::anyhow!("Mermaid Parse Error: {:?}", e))?;
    
    let mut graph = crate::graph::directed_graph::ast_to_graph(ast);
    crate::graph::cycle_removal::remove_cycles(&mut graph);
    
    let mut ranked = crate::layout::rank_assignment::assign_ranks(&graph);
    crate::layout::crossing_reduction::minimize_crossings(&mut ranked);
    let spaced = crate::layout::coordinate_assign::assign_coordinates(&ranked);
    
    Ok(render_to_typst(&spaced))
}

fn render_mindmap_to_typst(input: &str) -> anyhow::Result<String> {
    let mindmap = parse_mindmap_diagram(input)?;
    let width = 960.0;
    let height = 640.0;
    let center_x = width / 2.0;
    let center_y = height / 2.0;
    let horizontal_step = 180.0;
    let vertical_step = 72.0;

    let mut out = String::new();
    out.push_str(&format!(
        "#align(center)[\n  #box(width: {}pt, height: {}pt, clip: false)[\n",
        width, height
    ));

    let left_children: Vec<&MindmapNode> = mindmap.root.children.iter().step_by(2).collect();
    let right_children: Vec<&MindmapNode> = mindmap.root.children.iter().skip(1).step_by(2).collect();

    render_mindmap_node(&mut out, &mindmap.root.label, center_x - 70.0, center_y - 18.0, 140.0, true);

    for (idx, child) in left_children.iter().enumerate() {
        let child_y = center_y + ((idx as f64) - ((left_children.len().saturating_sub(1)) as f64 / 2.0)) * (vertical_step * 1.4);
        render_mindmap_branch(
            &mut out,
            &mindmap.root.label,
            center_x,
            center_y,
            child,
            center_x - horizontal_step,
            child_y,
            -1.0,
            1,
            horizontal_step,
            vertical_step,
        );
    }

    for (idx, child) in right_children.iter().enumerate() {
        let child_y = center_y + ((idx as f64) - ((right_children.len().saturating_sub(1)) as f64 / 2.0)) * (vertical_step * 1.4);
        render_mindmap_branch(
            &mut out,
            &mindmap.root.label,
            center_x,
            center_y,
            child,
            center_x + horizontal_step,
            child_y,
            1.0,
            1,
            horizontal_step,
            vertical_step,
        );
    }

    out.push_str("  ]\n]\n");
    Ok(out)
}

fn render_timeline_to_typst(input: &str) -> anyhow::Result<String> {
    let timeline = parse_timeline_diagram(input)?;
    let width = 760.0;
    let height = 120.0 + timeline.events.len() as f64 * 86.0;
    let axis_x = 180.0;

    let mut out = String::new();
    out.push_str(&format!(
        "#align(center)[\n  #box(width: {}pt, height: {}pt, clip: false)[\n",
        width, height
    ));

    if let Some(title) = &timeline.title {
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#text(weight: \"bold\", size: 13pt)[{}]]\n",
            escape_typst_text(title)
        ));
    }

    out.push_str(&format!(
        "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1.5pt + rgb(\"#888888\"), closed: false, ({}pt, 36pt), ({}pt, {}pt))]\n",
        axis_x,
        axis_x,
        height - 20.0
    ));

    for (idx, event) in timeline.events.iter().enumerate() {
        let y = 56.0 + idx as f64 * 86.0;
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#circle(radius: 5pt, fill: rgb(\"#3566a8\"))]\n",
            axis_x - 5.0,
            y - 5.0
        ));
        out.push_str(&format!(
            "  #place(dx: 20pt, dy: {}pt)[#text(weight: \"bold\", size: 10pt, fill: rgb(\"#3566a8\"))[{}]]\n",
            y - 8.0,
            escape_typst_text(&event.point)
        ));
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: 500pt, height: 46pt, radius: 6pt, fill: rgb(\"#eef4ff\"), stroke: 1pt + rgb(\"#9db9dd\"))[#pad(x: 8pt, y: 6pt)[#text(size: 9pt)[{}]]]]\n",
            axis_x + 24.0,
            y - 18.0,
            escape_typst_text(&event.description)
        ));
    }

    out.push_str("  ]\n]\n");
    Ok(out)
}

fn render_journey_to_typst(input: &str) -> anyhow::Result<String> {
    let journey = parse_journey_diagram(input)?;
    let width = 860.0;
    let lane_h = 74.0;
    let top = 44.0;
    let height = top + journey.steps.len() as f64 * lane_h + 60.0;
    let axis_start = 210.0;
    let axis_end = width - 40.0;
    let mut out = String::new();
    out.push_str(&format!(
        "#align(center)[\n  #box(width: {}pt, height: {}pt, clip: false)[\n",
        width, height
    ));

    if let Some(title) = &journey.title {
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#text(weight: \"bold\", size: 13pt)[{}]]\n",
            escape_typst_text(title)
        ));
    }

    for score in 1..=5 {
        let x = axis_start + (axis_end - axis_start) * ((score - 1) as f64 / 4.0);
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#text(size: 8pt, fill: rgb(\"#666666\"))[{}]]\n",
            x - 4.0,
            top - 8.0,
            score
        ));
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 0.8pt + rgb(\"#dddddd\"), closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
            x, top + 10.0, x, height - 26.0
        ));
    }

    for (idx, step) in journey.steps.iter().enumerate() {
        let y = top + idx as f64 * lane_h;
        out.push_str(&format!(
            "  #place(dx: 10pt, dy: {}pt)[#text(weight: \"bold\", size: 9pt)[{}]]\n",
            y + 6.0,
            escape_typst_text(&step.label)
        ));
        let x = axis_start + (axis_end - axis_start) * ((step.score - 1.0) / 4.0);
        let fill = journey_color(step.score);
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#circle(radius: 12pt, fill: rgb(\"{}\"), stroke: 1pt + rgb(\"#555555\"))]\n",
            x - 12.0,
            y + 2.0,
            fill
        ));
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#text(size: 8pt, fill: white)[{}]]\n",
            x - 4.0,
            y + 8.0,
            format!("{:.0}", step.score)
        ));
        if idx + 1 < journey.steps.len() {
            let next = &journey.steps[idx + 1];
            let next_x = axis_start + (axis_end - axis_start) * ((next.score - 1.0) / 4.0);
            out.push_str(&format!(
                "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1.3pt + rgb(\"#666666\"), closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
                x, y + 14.0, next_x, y + lane_h + 14.0
            ));
        }
        if !step.actors.is_empty() {
            out.push_str(&format!(
                "  #place(dx: {}pt, dy: {}pt)[#text(size: 8pt, fill: rgb(\"#666666\"))[{}]]\n",
                axis_end + 10.0,
                y + 6.0,
                escape_typst_text(&step.actors.join(", "))
            ));
        }
    }

    out.push_str("  ]\n]\n");
    Ok(out)
}

fn render_er_diagram_to_typst(input: &str) -> anyhow::Result<String> {
    let diagram = parse_er_diagram(input)?;
    let cols = 2usize.max(((diagram.entities.len() as f64).sqrt().ceil() as usize).max(1));
    let col_w = 280.0;
    let row_h = 180.0;
    let width = cols as f64 * col_w + 80.0;
    let rows = ((diagram.entities.len() + cols - 1) / cols).max(1);
    let height = rows as f64 * row_h + 80.0;

    let mut out = String::new();
    out.push_str(&format!(
        "#align(center)[\n  #box(width: {}pt, height: {}pt, clip: false)[\n",
        width, height
    ));

    let mut positions = std::collections::BTreeMap::new();
    for (idx, entity) in diagram.entities.iter().enumerate() {
        let col = idx % cols;
        let row = idx / cols;
        let x = 40.0 + col as f64 * col_w;
        let y = 30.0 + row as f64 * row_h;
        let body_h = (entity.attributes.len().max(1) as f64 * 16.0) + 18.0;
        let box_h = 28.0 + body_h;
        positions.insert(entity.name.clone(), (x, y, 210.0, box_h));

        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: 210pt, height: {}pt, radius: 6pt, fill: rgb(\"#edf7ed\"), stroke: 1.2pt + rgb(\"#2f7d32\"))[]]\n",
            x, y, box_h
        ));
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: 210pt, height: 28pt, radius: 6pt, fill: rgb(\"#dff0df\"), stroke: none)[#align(center + horizon)[#text(weight: \"bold\", size: 10pt)[{}]]]]\n",
            x, y, escape_typst_text(&entity.name)
        ));
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1pt + rgb(\"#2f7d32\"), closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
            x, y + 28.0, x + 210.0, y + 28.0
        ));

        for (attr_idx, attr) in entity.attributes.iter().enumerate() {
            out.push_str(&format!(
                "  #place(dx: {}pt, dy: {}pt)[#text(size: 9pt)[{}]]\n",
                x + 10.0,
                y + 40.0 + attr_idx as f64 * 16.0,
                escape_typst_text(attr)
            ));
        }
    }

    for rel in &diagram.relationships {
        let Some((fx, fy, fw, fh)) = positions.get(&rel.left).copied() else { continue };
        let Some((tx, ty, tw, _th)) = positions.get(&rel.right).copied() else { continue };
        let start_x = fx + fw / 2.0;
        let start_y = fy + fh;
        let end_x = tx + tw / 2.0;
        let end_y = ty;
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1.4pt + rgb(\"#555555\"), closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
            start_x, start_y, end_x, end_y
        ));
        let mid_x = (start_x + end_x) / 2.0;
        let mid_y = (start_y + end_y) / 2.0;
        let cardinality = format!("{} {}", rel.left_cardinality, rel.right_cardinality);
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#box(fill: white, inset: 2pt)[#text(size: 9pt, weight: \"bold\")[{}]]]\n",
            mid_x - 30.0,
            mid_y - 20.0,
            escape_typst_text(&cardinality)
        ));
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#box(fill: white, inset: 2pt)[#text(size: 9pt)[{}]]]\n",
            mid_x - 24.0,
            mid_y - 2.0,
            escape_typst_text(&rel.label)
        ));
    }

    out.push_str("  ]\n]\n");
    Ok(out)
}

fn render_pie_to_typst(input: &str) -> anyhow::Result<String> {
    let pie = parse_pie_diagram(input)?;
    let radius = 120.0;
    let center_x = 170.0;
    let center_y = 170.0;
    let width = 520.0;
    let height = 360.0;
    let colors = ["#4f81bd", "#c0504d", "#9bbb59", "#8064a2", "#4bacc6", "#f79646"];
    let total: f64 = pie.slices.iter().map(|s| s.value).sum::<f64>().max(1.0);
    let mut angle = -90.0f64;

    let mut out = String::new();
    out.push_str(&format!(
        "#align(center)[\n  #box(width: {}pt, height: {}pt, clip: false)[\n",
        width, height
    ));

    if let Some(title) = &pie.title {
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#text(weight: \"bold\", size: 13pt)[{}]]\n",
            escape_typst_text(title)
        ));
    }

    for (idx, slice) in pie.slices.iter().enumerate() {
        let sweep = 360.0 * (slice.value / total);
        let end_angle = angle + sweep;
        let start_rad = angle.to_radians();
        let end_rad = end_angle.to_radians();
        let x1 = center_x + radius * start_rad.cos();
        let y1 = center_y + radius * start_rad.sin();
        let x2 = center_x + radius * end_rad.cos();
        let y2 = center_y + radius * end_rad.sin();
        let large_arc = if sweep > 180.0 { 1 } else { 0 };
        let fill = colors[idx % colors.len()];
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#path(fill: rgb(\"{}\"), stroke: 1pt + white, closed: true, ({}pt, {}pt), ({}pt, {}pt), arc(({}pt, {}pt), radius: {}pt, large: {}, cw: true), ({}pt, {}pt))]\n",
            fill,
            center_x, center_y,
            x1, y1,
            x2, y2,
            radius,
            if large_arc == 1 { "true" } else { "false" },
            center_x, center_y
        ));
        let mid_angle = angle + sweep / 2.0;
        let label_x = center_x + (radius + 30.0) * mid_angle.to_radians().cos();
        let label_y = center_y + (radius + 12.0) * mid_angle.to_radians().sin();
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#text(size: 9pt)[{} ({})]]\n",
            label_x,
            label_y,
            escape_typst_text(&slice.label),
            format!("{:.0}", slice.value)
        ));
        angle = end_angle;
    }

    out.push_str("  ]\n]\n");
    Ok(out)
}

fn render_class_diagram_to_typst(input: &str) -> anyhow::Result<String> {
    let diagram = parse_class_diagram(input)?;
    let cols = 2usize.max(((diagram.classes.len() as f64).sqrt().ceil() as usize).max(1));
    let col_w = 260.0;
    let row_h = 170.0;
    let width = cols as f64 * col_w + 80.0;
    let rows = ((diagram.classes.len() + cols - 1) / cols).max(1);
    let height = rows as f64 * row_h + 80.0;

    let mut out = String::new();
    out.push_str(&format!(
        "#align(center)[\n  #box(width: {}pt, height: {}pt, clip: false)[\n",
        width, height
    ));

    let mut positions = std::collections::BTreeMap::new();
    for (idx, class) in diagram.classes.iter().enumerate() {
        let col = idx % cols;
        let row = idx / cols;
        let x = 40.0 + col as f64 * col_w;
        let y = 30.0 + row as f64 * row_h;
        let body_h = (class.members.len().max(1) as f64 * 16.0) + 16.0;
        let box_h = 28.0 + body_h;
        positions.insert(class.name.clone(), (x, y, 190.0, box_h));

        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: 190pt, height: {}pt, radius: 6pt, fill: rgb(\"#eef4ff\"), stroke: 1.2pt + rgb(\"#3566a8\"))[]]\n",
            x, y, box_h
        ));
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: 190pt, height: 28pt, radius: 6pt, fill: rgb(\"#dfeafc\"), stroke: none)[#align(center + horizon)[#text(weight: \"bold\", size: 10pt)[{}]]]]\n",
            x, y, escape_typst_text(&class.name)
        ));
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1pt + rgb(\"#3566a8\"), closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
            x, y + 28.0, x + 190.0, y + 28.0
        ));

        for (member_idx, member) in class.members.iter().enumerate() {
            out.push_str(&format!(
                "  #place(dx: {}pt, dy: {}pt)[#text(size: 9pt)[{}]]\n",
                x + 10.0,
                y + 40.0 + member_idx as f64 * 16.0,
                escape_typst_text(member)
            ));
        }
    }

    for rel in &diagram.relationships {
        let Some((fx, fy, fw, fh)) = positions.get(&rel.from).copied() else { continue };
        let Some((tx, ty, tw, _th)) = positions.get(&rel.to).copied() else { continue };
        let start_x = fx + fw / 2.0;
        let start_y = fy + fh;
        let end_x = tx + tw / 2.0;
        let end_y = ty;
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: {}, closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
            class_relation_stroke(&rel.kind),
            start_x, start_y, end_x, end_y
        ));
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#text(size: 12pt, fill: rgb(\"#555555\"))[{}]]\n",
            end_x - 5.0,
            end_y - 8.0,
            relation_arrow_glyph(&rel.kind)
        ));
        if let Some(label) = &rel.label {
            out.push_str(&format!(
                "  #place(dx: {}pt, dy: {}pt)[#box(fill: white, inset: 2pt)[#text(size: 9pt)[{}]]]\n",
                ((start_x + end_x) / 2.0) + 6.0,
                ((start_y + end_y) / 2.0) - 6.0,
                escape_typst_text(label)
            ));
        }
    }

    out.push_str("  ]\n]\n");
    Ok(out)
}

fn render_gantt_to_typst(input: &str) -> anyhow::Result<String> {
    let gantt = parse_gantt_diagram(input)?;
    let label_w = 180.0;
    let chart_w = 540.0;
    let top = 34.0;
    let row_h = 32.0;
    let section_gap = 14.0;
    let total_rows = gantt.rows.len() as f64;
    let width = label_w + chart_w + 60.0;
    let height = top + 60.0 + total_rows * row_h + gantt.section_count as f64 * section_gap + 30.0;
    let max_end = gantt.rows.iter().map(|r| r.start + r.duration).max().unwrap_or(1) as f64;
    let scale = if max_end > 0.0 { chart_w / max_end } else { chart_w };

    let mut out = String::new();
    out.push_str(&format!(
        "#align(center)[\n  #box(width: {}pt, height: {}pt, clip: false)[\n",
        width, height
    ));

    if let Some(title) = &gantt.title {
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#text(weight: \"bold\", size: 13pt)[{}]]\n",
            escape_typst_text(title)
        ));
    }

    for i in 0..=(max_end as usize) {
        let x = label_w + 20.0 + (i as f64 * scale);
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 0.6pt + rgb(\"#dddddd\"), closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
            x, top + 18.0, x, height - 12.0
        ));
    }

    let mut y = top + 24.0;
    let palette = ["#e8f1fb", "#edf7ed", "#fff7e8", "#fceef6"];
    let mut section_index = 0usize;
    for row in &gantt.rows {
        if let Some(section) = &row.section {
            if row.section_starts_here {
                out.push_str(&format!(
                    "  #place(dx: 0pt, dy: {}pt)[#text(weight: \"bold\", size: 10pt, fill: rgb(\"#555555\"))[{}]]\n",
                    y - 14.0,
                    escape_typst_text(section)
                ));
                y += 10.0;
                section_index += 1;
            }
        }

        out.push_str(&format!(
            "  #place(dx: 0pt, dy: {}pt)[#text(size: 9pt)[{}]]\n",
            y + 5.0,
            escape_typst_text(&row.label)
        ));
        let bar_x = label_w + 20.0 + row.start as f64 * scale;
        let bar_w = (row.duration.max(1) as f64 * scale).max(18.0);
        let fill = palette[(section_index.saturating_sub(1)) % palette.len()];
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: {}pt, height: 18pt, radius: 4pt, fill: rgb(\"{}\"), stroke: 1pt + rgb(\"#777777\"))[#align(center + horizon)[#text(size: 8pt)[{}]]]]\n",
            bar_x, y, bar_w, fill, escape_typst_text(&row.label)
        ));
        y += row_h;
    }

    out.push_str("  ]\n]\n");
    Ok(out)
}

fn render_state_diagram_to_typst(input: &str) -> anyhow::Result<String> {
    let state = parse_state_diagram(input)?;
    let width = 640.0;
    let height = 120.0 + state.states.len() as f64 * 90.0;
    let state_w = 160.0;
    let state_h = 32.0;
    let x = 220.0;

    let mut out = String::new();
    out.push_str(&format!(
        "#align(center)[\n  #box(width: {}pt, height: {}pt, clip: false)[\n",
        width, height
    ));

    for (idx, name) in state.states.iter().enumerate() {
        let y = 40.0 + idx as f64 * 90.0;
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: {}pt, height: {}pt, radius: 8pt, fill: rgb(\"#eef4ff\"), stroke: 1.2pt + rgb(\"#3566a8\"))[#align(center + horizon)[#text(size: 10pt)[{}]]]]\n",
            x, y, state_w, state_h, escape_typst_text(name)
        ));
    }

    for transition in &state.transitions {
        let Some(from_idx) = state.index_of(&transition.from) else { continue };
        let Some(to_idx) = state.index_of(&transition.to) else { continue };
        let y1 = 40.0 + from_idx as f64 * 90.0 + state_h;
        let y2 = 40.0 + to_idx as f64 * 90.0;
        let mid_x = x + state_w / 2.0;
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1.4pt + rgb(\"#555555\"), closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
            mid_x, y1, mid_x, y2
        ));
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#text(size: 12pt, fill: rgb(\"#555555\"))[▼]]\n",
            mid_x - 5.0, y2 - 8.0
        ));
        if let Some(label) = &transition.label {
            out.push_str(&format!(
                "  #place(dx: {}pt, dy: {}pt)[#box(fill: white, inset: 2pt)[#text(size: 9pt)[{}]]]\n",
                mid_x + 8.0, ((y1 + y2) / 2.0) - 6.0, escape_typst_text(label)
            ));
        }
    }

    out.push_str("  ]\n]\n");
    Ok(out)
}

fn render_sequence_to_typst(input: &str) -> anyhow::Result<String> {
    let seq = parse_sequence_diagram(input)?;
    let activations = compute_sequence_activations(&seq);
    let lane_spacing = 150.0;
    let top = 40.0;
    let header_h = 34.0;
    let row_h = 38.0;
    let width = (seq.participants.len().max(1) as f64 - 1.0) * lane_spacing + 160.0;
    let height = top + header_h + (seq.rows.len() as f64 * row_h) + 80.0;

    let mut out = String::new();
    out.push_str(&format!(
        "#align(center)[\n  #scale(x: 100%, y: 100%, reflow: true)[\n    #box(width: {}pt, height: {}pt, clip: false)[\n",
        width, height
    ));

    for (idx, participant) in seq.participants.iter().enumerate() {
        let x = 60.0 + (idx as f64 * lane_spacing);
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: 110pt, height: 24pt, radius: 6pt, fill: rgb(\"#eef4ff\"), stroke: 1.2pt + rgb(\"#3566a8\"))[#align(center + horizon)[#text(size: 10pt)[{}]]]]\n",
            x - 55.0,
            top,
            escape_typst_text(&participant.label),
        ));
        out.push_str(&format!(
            "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1pt + rgb(\"#bbbbbb\"), closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
            x, top + 24.0, x, height - 20.0
        ));
    }

    for activation in &activations {
        let Some(idx) = seq.index_of(&activation.participant) else { continue };
        let x = 60.0 + (idx as f64 * lane_spacing) - 7.0 + (activation.depth as f64 * 6.0);
        let y = top + header_h + (activation.start_row as f64 * row_h) - 10.0;
        let activation_height = ((activation.end_row - activation.start_row) as f64 * row_h).max(24.0);
        out.push_str(&format!(
            "  #place(dx: {}pt, dy: {}pt)[#rect(width: 14pt, height: {}pt, radius: 3pt, fill: rgb(\"#fff4d6\"), stroke: 1pt + rgb(\"#b26b00\"))[]]\n",
            x,
            y,
            activation_height
        ));
    }

    for (row_idx, row) in seq.rows.iter().enumerate() {
        let y = top + header_h + (row_idx as f64 * row_h);
        match row {
            SequenceRow::Message { from, to, label } => {
                let Some(from_idx) = seq.index_of(from) else { continue };
                let Some(to_idx) = seq.index_of(to) else { continue };
                let x1 = 60.0 + (from_idx as f64 * lane_spacing);
                let x2 = 60.0 + (to_idx as f64 * lane_spacing);
                out.push_str(&format!(
                    "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1.4pt + rgb(\"#555555\"), closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
                    x1, y, x2, y
                ));
                out.push_str(&format!(
                    "  #place(dx: {}pt, dy: {}pt)[#text(size: 12pt, fill: rgb(\"#555555\"))[{}]]\n",
                    x2 - 8.0,
                    y - 8.0,
                    if x2 >= x1 { "▶" } else { "◀" }
                ));
                let mid = (x1 + x2) / 2.0;
                out.push_str(&format!(
                    "  #place(dx: {}pt, dy: {}pt)[#box(fill: white, inset: 2pt)[#text(size: 9pt)[{}]]]\n",
                    mid - 45.0,
                    y - 20.0,
                    escape_typst_text(label)
                ));
            }
            SequenceRow::Note { over, label } => {
                let indices: Vec<usize> = over.iter().filter_map(|id| seq.index_of(id)).collect();
                if indices.is_empty() {
                    continue;
                }
                let left = 60.0 + (*indices.iter().min().unwrap() as f64 * lane_spacing) - 40.0;
                let right = 60.0 + (*indices.iter().max().unwrap() as f64 * lane_spacing) + 40.0;
                out.push_str(&format!(
                    "  #place(dx: {}pt, dy: {}pt)[#rect(width: {}pt, height: 24pt, radius: 5pt, fill: rgb(\"#fff7e8\"), stroke: 1pt + rgb(\"#b26b00\"))[#align(center + horizon)[#text(size: 9pt)[{}]]]]\n",
                    left,
                    y - 12.0,
                    right - left,
                    escape_typst_text(label)
                ));
            }
            SequenceRow::Activate(participant) => {
                out.push_str(&format!(
                    "  #place(dx: 24pt, dy: {}pt)[#text(size: 9pt, fill: rgb(\"#8a5a00\"), style: \"italic\")[activate {}]]\n",
                    y - 10.0,
                    escape_typst_text(participant)
                ));
            }
            SequenceRow::Deactivate(participant) => {
                out.push_str(&format!(
                    "  #place(dx: 24pt, dy: {}pt)[#text(size: 9pt, fill: rgb(\"#8a5a00\"), style: \"italic\")[deactivate {}]]\n",
                    y - 10.0,
                    escape_typst_text(participant)
                ));
            }
            SequenceRow::Control(label) => {
                out.push_str(&format!(
                    "  #place(dx: 24pt, dy: {}pt)[#text(size: 9pt, fill: rgb(\"#666666\"), style: \"italic\")[{}]]\n",
                    y - 10.0,
                    escape_typst_text(label)
                ));
            }
        }
    }

    out.push_str("    ]\n  ]\n]\n");
    Ok(out)
}

fn typst_color(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.starts_with('#') {
        Some(format!("rgb(\"{}\")", value))
    } else {
        Some(value.to_string())
    }
}

fn typst_stroke(color: &str, width: &str) -> String {
    let width_num = width.trim_end_matches("px").trim();
    let color_expr = typst_color(Some(color)).unwrap_or_else(|| "rgb(\"#333333\")".to_string());
    format!("{}pt + {}", width_num, color_expr)
}

fn edge_stroke(edge: &crate::layout::coordinate_assign::EdgeGeometry) -> String {
    let width = match edge.style {
        crate::parsing::lexer::EdgeStyle::ThickArrow => "2.5pt",
        _ => "1.5pt",
    };
    format!("{} + rgb(\"#555555\")", width)
}

fn has_arrowhead(style: crate::parsing::lexer::EdgeStyle) -> bool {
    style != crate::parsing::lexer::EdgeStyle::SolidLine
}

fn edge_label_anchor(path: &[(f64, f64)]) -> (f64, f64) {
    if path.len() < 2 {
        return (0.0, 0.0);
    }
    let mid_segment = (path.len() - 1) / 2;
    let start = path[mid_segment];
    let end = path[mid_segment + 1];
    ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0)
}

fn trim_path_end(path: &[(f64, f64)], distance: f64) -> Vec<(f64, f64)> {
    if path.len() < 2 {
        return path.to_vec();
    }

    let mut trimmed = path.to_vec();
    for idx in (1..trimmed.len()).rev() {
        let start = trimmed[idx - 1];
        let end = trimmed[idx];
        let dx = end.0 - start.0;
        let dy = end.1 - start.1;
        let magnitude = (dx * dx + dy * dy).sqrt();
        if magnitude < 0.001 {
            continue;
        }

        let applied = distance.min(magnitude * 0.6);
        let ux = dx / magnitude;
        let uy = dy / magnitude;
        trimmed[idx] = (end.0 - (ux * applied), end.1 - (uy * applied));
        return trimmed;
    }

    trimmed
}

fn arrowhead_points(path: &[(f64, f64)], length: f64, width: f64) -> Option<[(f64, f64); 3]> {
    if path.len() < 2 {
        return None;
    }

    for segment in path.windows(2).rev() {
        let start = segment[0];
        let end = segment[1];
        let dx = end.0 - start.0;
        let dy = end.1 - start.1;
        let magnitude = (dx * dx + dy * dy).sqrt();
        if magnitude < 0.001 {
            continue;
        }

        let ux = dx / magnitude;
        let uy = dy / magnitude;
        let px = -uy;
        let py = ux;
        let base_x = end.0 - (ux * length);
        let base_y = end.1 - (uy * length);
        let half_width = width / 2.0;

        return Some([
            end,
            (base_x + (px * half_width), base_y + (py * half_width)),
            (base_x - (px * half_width), base_y - (py * half_width)),
        ]);
    }

    None
}

fn escape_typst_text(input: &str) -> String {
    input.replace('\\', "\\\\").replace('[', "\\[").replace(']', "\\]")
}

#[derive(Debug)]
struct SequenceDiagram {
    participants: Vec<SequenceParticipant>,
    rows: Vec<SequenceRow>,
}

impl SequenceDiagram {
    fn index_of(&self, id: &str) -> Option<usize> {
        self.participants.iter().position(|p| p.id == id)
    }
}

#[derive(Debug)]
struct SequenceParticipant {
    id: String,
    label: String,
}

#[derive(Debug)]
enum SequenceRow {
    Message { from: String, to: String, label: String },
    Note { over: Vec<String>, label: String },
    Activate(String),
    Deactivate(String),
    Control(String),
}

#[derive(Debug)]
struct SequenceActivation {
    participant: String,
    start_row: usize,
    end_row: usize,
    depth: usize,
}

#[derive(Debug)]
struct ClassDiagram {
    classes: Vec<ClassNode>,
    relationships: Vec<ClassRelationship>,
}

#[derive(Debug)]
struct ClassNode {
    name: String,
    members: Vec<String>,
}

#[derive(Debug)]
struct ClassRelationship {
    from: String,
    to: String,
    kind: String,
    label: Option<String>,
}

#[derive(Debug)]
struct GanttDiagram {
    title: Option<String>,
    rows: Vec<GanttRow>,
    section_count: usize,
}

#[derive(Debug)]
struct GanttRow {
    section: Option<String>,
    section_starts_here: bool,
    label: String,
    start: i32,
    duration: i32,
}

#[derive(Debug)]
struct StateDiagram {
    states: Vec<String>,
    transitions: Vec<StateTransition>,
}

impl StateDiagram {
    fn index_of(&self, name: &str) -> Option<usize> {
        self.states.iter().position(|s| s == name)
    }
}

#[derive(Debug)]
struct StateTransition {
    from: String,
    to: String,
    label: Option<String>,
}

#[derive(Debug)]
struct ErDiagram {
    entities: Vec<ErEntity>,
    relationships: Vec<ErRelationship>,
}

#[derive(Debug)]
struct ErEntity {
    name: String,
    attributes: Vec<String>,
}

#[derive(Debug)]
struct ErRelationship {
    left: String,
    left_cardinality: String,
    right_cardinality: String,
    right: String,
    label: String,
}

#[derive(Debug)]
struct PieDiagram {
    title: Option<String>,
    slices: Vec<PieSlice>,
}

#[derive(Debug)]
struct PieSlice {
    label: String,
    value: f64,
}

#[derive(Debug)]
struct TimelineDiagram {
    title: Option<String>,
    events: Vec<TimelineEvent>,
}

#[derive(Debug)]
struct TimelineEvent {
    point: String,
    description: String,
}

#[derive(Debug)]
struct JourneyDiagram {
    title: Option<String>,
    steps: Vec<JourneyStep>,
}

#[derive(Debug)]
struct JourneyStep {
    label: String,
    score: f64,
    actors: Vec<String>,
}

#[derive(Debug)]
struct MindmapDiagram {
    root: MindmapNode,
}

#[derive(Debug, Clone)]
struct MindmapNode {
    label: String,
    children: Vec<MindmapNode>,
}

fn parse_sequence_diagram(input: &str) -> anyhow::Result<SequenceDiagram> {
    let mut participants = Vec::new();
    let mut rows = Vec::new();

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line == "sequenceDiagram" {
            continue;
        }
        if let Some(rest) = line.strip_prefix("participant ") {
            if let Some((id, label)) = rest.split_once(" as ") {
                participants.push(SequenceParticipant {
                    id: id.trim().to_string(),
                    label: label.trim().to_string(),
                });
            } else {
                let id = rest.trim().to_string();
                participants.push(SequenceParticipant { label: id.clone(), id });
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("Note over ") {
            if let Some((actors, label)) = rest.split_once(':') {
                rows.push(SequenceRow::Note {
                    over: actors.split(',').map(|s| s.trim().to_string()).collect(),
                    label: label.trim().to_string(),
                });
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("loop ") {
            rows.push(SequenceRow::Control(format!("Loop: {}", rest.trim())));
            continue;
        }
        if let Some(rest) = line.strip_prefix("alt ") {
            rows.push(SequenceRow::Control(format!("Alt: {}", rest.trim())));
            continue;
        }
        if let Some(rest) = line.strip_prefix("else ") {
            rows.push(SequenceRow::Control(format!("Else: {}", rest.trim())));
            continue;
        }
        if let Some(rest) = line.strip_prefix("par ") {
            rows.push(SequenceRow::Control(format!("Par: {}", rest.trim())));
            continue;
        }
        if let Some(rest) = line.strip_prefix("and ") {
            rows.push(SequenceRow::Control(format!("And: {}", rest.trim())));
            continue;
        }
        if let Some(rest) = line.strip_prefix("critical ") {
            rows.push(SequenceRow::Control(format!("Critical: {}", rest.trim())));
            continue;
        }
        if let Some(rest) = line.strip_prefix("option ") {
            rows.push(SequenceRow::Control(format!("Option: {}", rest.trim())));
            continue;
        }
        if let Some(rest) = line.strip_prefix("break ") {
            rows.push(SequenceRow::Control(format!("Break: {}", rest.trim())));
            continue;
        }
        if let Some(rest) = line.strip_prefix("rect ") {
            rows.push(SequenceRow::Control(format!("Rect: {}", rest.trim())));
            continue;
        }
        if let Some(rest) = line.strip_prefix("activate ") {
            rows.push(SequenceRow::Activate(rest.trim().to_string()));
            continue;
        }
        if let Some(rest) = line.strip_prefix("deactivate ") {
            rows.push(SequenceRow::Deactivate(rest.trim().to_string()));
            continue;
        }
        if line == "end" {
            rows.push(SequenceRow::Control("End".to_string()));
            continue;
        }

        if let Some((lhs, label)) = line.split_once(':') {
            let arrow_patterns = ["-->>", "->>", "-->", "->"];
            if let Some(arrow) = arrow_patterns.iter().find(|p| lhs.contains(**p)) {
                if let Some((from, to)) = lhs.split_once(*arrow) {
                    rows.push(SequenceRow::Message {
                        from: from.trim().to_string(),
                        to: to.trim().to_string(),
                        label: label.trim().to_string(),
                    });
                    continue;
                }
            }
        }
    }

    if participants.is_empty() {
        return Err(anyhow::anyhow!("Sequence diagram has no participants"));
    }

    Ok(SequenceDiagram { participants, rows })
}

fn compute_sequence_activations(seq: &SequenceDiagram) -> Vec<SequenceActivation> {
    let mut activations = Vec::new();
    let mut stacks: FxHashMap<String, Vec<(usize, usize)>> = FxHashMap::default();

    for (row_idx, row) in seq.rows.iter().enumerate() {
        match row {
            SequenceRow::Activate(participant) => {
                let stack = stacks.entry(participant.clone()).or_default();
                let depth = stack.len();
                stack.push((row_idx, depth));
            }
            SequenceRow::Deactivate(participant) => {
                if let Some(stack) = stacks.get_mut(participant) {
                    if let Some((start_row, depth)) = stack.pop() {
                        activations.push(SequenceActivation {
                            participant: participant.clone(),
                            start_row,
                            end_row: row_idx.max(start_row + 1),
                            depth,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    for (participant, stack) in stacks {
        for (start_row, depth) in stack {
            activations.push(SequenceActivation {
                participant: participant.clone(),
                start_row,
                end_row: seq.rows.len().max(start_row + 1),
                depth,
            });
        }
    }

    activations
}

fn parse_gantt_diagram(input: &str) -> anyhow::Result<GanttDiagram> {
    let mut title = None;
    let mut rows = Vec::new();
    let mut current_section: Option<String> = None;
    let mut section_count = 0usize;

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line == "gantt" || line.starts_with("dateFormat") || line.starts_with("axisFormat") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("title ") {
            title = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("section ") {
            current_section = Some(rest.trim().to_string());
            section_count += 1;
            continue;
        }

        if let Some((label, rhs)) = line.split_once(':') {
            let parts: Vec<&str> = rhs.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            let numeric: Vec<i32> = parts.iter().filter_map(|p| p.parse::<i32>().ok()).collect();
            let (start, duration) = match numeric.as_slice() {
                [start, duration, ..] => (*start, *duration),
                [single] => (0, *single),
                [] => (0, 1),
            };
            let section_starts_here = rows.last().map(|r: &GanttRow| r.section != current_section).unwrap_or(true);
            rows.push(GanttRow {
                section: current_section.clone(),
                section_starts_here,
                label: label.trim().to_string(),
                start,
                duration,
            });
        }
    }

    if rows.is_empty() {
        return Err(anyhow::anyhow!("Gantt diagram has no rows"));
    }

    Ok(GanttDiagram { title, rows, section_count })
}

fn parse_state_diagram(input: &str) -> anyhow::Result<StateDiagram> {
    let mut states: Vec<String> = Vec::new();
    let mut transitions = Vec::new();

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("stateDiagram") {
            continue;
        }

        if let Some((lhs, rhs)) = line.split_once("-->") {
            let from = lhs.trim().to_string();
            let (to, label) = if let Some((to, label)) = rhs.split_once(':') {
                (to.trim().to_string(), Some(label.trim().to_string()))
            } else {
                (rhs.trim().to_string(), None)
            };
            if from != "[*]" && !states.contains(&from) {
                states.push(from.clone());
            }
            if to != "[*]" && !states.contains(&to) {
                states.push(to.clone());
            }
            if from != "[*]" && to != "[*]" {
                transitions.push(StateTransition { from, to, label });
            }
        }
    }

    if states.is_empty() {
        return Err(anyhow::anyhow!("State diagram has no states"));
    }

    Ok(StateDiagram { states, transitions })
}

fn parse_class_diagram(input: &str) -> anyhow::Result<ClassDiagram> {
    let mut classes: std::collections::BTreeMap<String, ClassNode> = std::collections::BTreeMap::new();
    let mut relationships = Vec::new();
    let mut current_class: Option<String> = None;

    let relation_tokens = [
        "<|--", "--|>", "*--", "--*", "o--", "--o", "<..", "..>", "<--", "-->", "..", "--",
    ];

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("classDiagram") {
            continue;
        }

        if let Some(name) = current_class.clone() {
            if line == "}" {
                current_class = None;
                continue;
            }
            if let Some(class_node) = classes.get_mut(&name) {
                class_node.members.push(line.to_string());
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("class ") {
            if let Some(name) = rest.strip_suffix('{') {
                let class_name = name.trim().to_string();
                classes.entry(class_name.clone()).or_insert_with(|| ClassNode {
                    name: class_name.clone(),
                    members: Vec::new(),
                });
                current_class = Some(class_name);
                continue;
            }

            let class_name = rest.trim().to_string();
            classes.entry(class_name.clone()).or_insert_with(|| ClassNode {
                name: class_name,
                members: Vec::new(),
            });
            continue;
        }

        if let Some((token, idx)) = relation_tokens
            .iter()
            .filter_map(|token| line.find(token).map(|idx| (*token, idx)))
            .min_by_key(|(_, idx)| *idx)
        {
            let left = line[..idx].trim().to_string();
            let right_part = line[idx + token.len()..].trim();
            let (right, label) = if let Some((dest, lbl)) = right_part.split_once(':') {
                (dest.trim().to_string(), Some(lbl.trim().to_string()))
            } else {
                (right_part.to_string(), None)
            };

            if !left.is_empty() {
                classes.entry(left.clone()).or_insert_with(|| ClassNode { name: left.clone(), members: Vec::new() });
            }
            if !right.is_empty() {
                classes.entry(right.clone()).or_insert_with(|| ClassNode { name: right.clone(), members: Vec::new() });
            }

            relationships.push(ClassRelationship {
                from: left,
                to: right,
                kind: token.to_string(),
                label,
            });
        }
    }

    if classes.is_empty() {
        return Err(anyhow::anyhow!("Class diagram has no classes"));
    }

    Ok(ClassDiagram {
        classes: classes.into_values().collect(),
        relationships,
    })
}

fn parse_er_diagram(input: &str) -> anyhow::Result<ErDiagram> {
    let mut entities: std::collections::BTreeMap<String, ErEntity> = std::collections::BTreeMap::new();
    let mut relationships = Vec::new();
    let mut current_entity: Option<String> = None;

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("erDiagram") {
            continue;
        }

        if let Some(name) = current_entity.clone() {
            if line == "}" {
                current_entity = None;
                continue;
            }
            if let Some(entity) = entities.get_mut(&name) {
                entity.attributes.push(line.to_string());
            }
            continue;
        }

        if let Some(name) = line.strip_suffix('{') {
            let entity_name = name.trim().to_string();
            entities.entry(entity_name.clone()).or_insert_with(|| ErEntity {
                name: entity_name.clone(),
                attributes: Vec::new(),
            });
            current_entity = Some(entity_name);
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[1].contains("||") || parts.len() >= 4 && parts[1].contains('}') || parts[1].contains('|') || parts[1].contains('o') {
            if parts.len() >= 4 {
                let left = parts[0].to_string();
                let cardinality = parts[1];
                let right = parts[2].to_string();
                let label = parts[3..].join(" ").trim_matches(':').trim().to_string();
                let (left_cardinality, right_cardinality) = if let Some((l, r)) = cardinality.split_once("--") {
                    (l.to_string(), r.to_string())
                } else {
                    (cardinality.to_string(), String::new())
                };
                entities.entry(left.clone()).or_insert_with(|| ErEntity { name: left.clone(), attributes: Vec::new() });
                entities.entry(right.clone()).or_insert_with(|| ErEntity { name: right.clone(), attributes: Vec::new() });
                relationships.push(ErRelationship {
                    left,
                    left_cardinality,
                    right_cardinality,
                    right,
                    label,
                });
            }
        }
    }

    if entities.is_empty() {
        return Err(anyhow::anyhow!("ER diagram has no entities"));
    }

    Ok(ErDiagram { entities: entities.into_values().collect(), relationships })
}

fn parse_pie_diagram(input: &str) -> anyhow::Result<PieDiagram> {
    let mut title = None;
    let mut slices = Vec::new();

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line == "pie" || line.starts_with("showData") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("title ") {
            title = Some(rest.trim().to_string());
            continue;
        }
        if let Some((label, value)) = line.split_once(':') {
            let label = label.trim().trim_matches('"').to_string();
            if let Ok(value) = value.trim().parse::<f64>() {
                slices.push(PieSlice { label, value });
            }
        }
    }

    if slices.is_empty() {
        return Err(anyhow::anyhow!("Pie diagram has no slices"));
    }

    Ok(PieDiagram { title, slices })
}

fn parse_timeline_diagram(input: &str) -> anyhow::Result<TimelineDiagram> {
    let mut title = None;
    let mut current_point: Option<String> = None;
    let mut events = Vec::new();

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line == "timeline" {
            continue;
        }
        if let Some(rest) = line.strip_prefix("title ") {
            title = Some(rest.trim().to_string());
            continue;
        }
        if let Some((point, desc)) = line.split_once(':') {
            events.push(TimelineEvent {
                point: point.trim().to_string(),
                description: desc.trim().to_string(),
            });
            current_point = None;
            continue;
        }
        if current_point.is_none() {
            current_point = Some(line.to_string());
        } else if let Some(point) = current_point.take() {
            events.push(TimelineEvent {
                point,
                description: line.to_string(),
            });
        }
    }

    if events.is_empty() {
        return Err(anyhow::anyhow!("Timeline diagram has no events"));
    }

    Ok(TimelineDiagram { title, events })
}

fn parse_journey_diagram(input: &str) -> anyhow::Result<JourneyDiagram> {
    let mut title = None;
    let mut steps = Vec::new();

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line == "journey" || line.starts_with("section ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("title ") {
            title = Some(rest.trim().to_string());
            continue;
        }
        if let Some((label, rhs)) = line.split_once(':') {
            let mut parts = rhs.split(':').map(|s| s.trim());
            let score = parts.next().and_then(|s| s.parse::<f64>().ok()).unwrap_or(3.0);
            let actors = parts
                .next()
                .map(|s| s.split(',').map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect())
                .unwrap_or_else(Vec::new);
            steps.push(JourneyStep {
                label: label.trim().to_string(),
                score,
                actors,
            });
        }
    }

    if steps.is_empty() {
        return Err(anyhow::anyhow!("Journey diagram has no steps"));
    }

    Ok(JourneyDiagram { title, steps })
}

fn parse_mindmap_diagram(input: &str) -> anyhow::Result<MindmapDiagram> {
    let mut entries: Vec<(usize, String)> = Vec::new();

    for raw_line in input.lines() {
        if raw_line.trim().is_empty() || raw_line.trim() == "mindmap" {
            continue;
        }
        let indent = raw_line.chars().take_while(|c| c.is_whitespace()).count();
        let level = indent / 2;
        let label = raw_line.trim().trim_start_matches("::").trim().to_string();
        if !label.is_empty() {
            entries.push((level, label));
        }
    }

    if entries.is_empty() {
        return Err(anyhow::anyhow!("Mindmap has no nodes"));
    }

    fn build(entries: &[(usize, String)], idx: &mut usize, level: usize) -> MindmapNode {
        let (_, label) = &entries[*idx];
        let mut node = MindmapNode { label: label.clone(), children: Vec::new() };
        *idx += 1;
        while *idx < entries.len() {
            let (next_level, _) = &entries[*idx];
            if *next_level <= level {
                break;
            }
            node.children.push(build(entries, idx, *next_level));
        }
        node
    }

    let mut idx = 0;
    let root = build(&entries, &mut idx, entries[0].0);
    Ok(MindmapDiagram { root })
}

fn journey_color(score: f64) -> &'static str {
    if score >= 4.5 {
        "#2f7d32"
    } else if score >= 3.5 {
        "#7cb342"
    } else if score >= 2.5 {
        "#f9a825"
    } else {
        "#c62828"
    }
}

fn render_mindmap_branch(
    out: &mut String,
    _root_label: &str,
    parent_x: f64,
    parent_y: f64,
    node: &MindmapNode,
    x: f64,
    y: f64,
    direction: f64,
    depth: usize,
    horizontal_step: f64,
    vertical_step: f64,
) {
    let node_w = (node.label.len() as f64 * 6.2).clamp(80.0, 170.0);
    let node_h = 28.0;
    let node_x = x - node_w / 2.0;
    let node_y = y - node_h / 2.0;

    out.push_str(&format!(
        "  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1.2pt + rgb(\"#666666\"), closed: false, ({}pt, {}pt), ({}pt, {}pt))]\n",
        parent_x, parent_y, x, y
    ));
    render_mindmap_node(out, &node.label, node_x, node_y, node_w, false);

    if node.children.is_empty() {
        return;
    }

    let count = node.children.len();
    for (idx, child) in node.children.iter().enumerate() {
        let child_y = y + ((idx as f64) - ((count.saturating_sub(1)) as f64 / 2.0)) * vertical_step;
        let child_x = x + direction * (horizontal_step / (1.0 + depth as f64 * 0.15));
        render_mindmap_branch(
            out,
            &node.label,
            x,
            y,
            child,
            child_x,
            child_y,
            direction,
            depth + 1,
            horizontal_step,
            vertical_step * 0.82,
        );
    }
}

fn render_mindmap_node(out: &mut String, label: &str, x: f64, y: f64, width: f64, is_root: bool) {
    let (fill, stroke, text_weight) = if is_root {
        ("#dfeafc", "#3566a8", "bold")
    } else {
        ("#eef4ff", "#7fa6d6", "regular")
    };
    out.push_str(&format!(
        "  #place(dx: {}pt, dy: {}pt)[#rect(width: {}pt, height: 28pt, radius: 14pt, fill: rgb(\"{}\"), stroke: 1pt + rgb(\"{}\"))[#align(center + horizon)[#text(size: 9pt, weight: \"{}\")[{}]]]]\n",
        x,
        y,
        width,
        fill,
        stroke,
        text_weight,
        escape_typst_text(label)
    ));
}

fn class_relation_stroke(kind: &str) -> String {
    if kind.contains("..") {
        "1.2pt + rgb(\"#777777\")".to_string()
    } else {
        "1.4pt + rgb(\"#555555\")".to_string()
    }
}

fn relation_arrow_glyph(kind: &str) -> &'static str {
    match kind {
        "<|--" | "--|>" => "△",
        "*--" | "--*" => "◆",
        "o--" | "--o" => "◇",
        "<.." | "..>" | "<--" | "-->" => "▼",
        _ => "•",
    }
}

#[cfg(test)]
mod tests {
    use super::mermaid_to_typst;

    #[test]
    fn renders_prd_local_global_duty_cycle_flowchart() {
        let src = r#"flowchart LR
    classDef local fill:#eef4ff,stroke:#3566a8,stroke-width:2px;
    classDef global fill:#edf7ed,stroke:#2f7d32,stroke-width:2px;
    classDef guard fill:#fff1f1,stroke:#b42318,stroke-width:2px;

    A[Pulse Width<br/>10 us to 4 ms]:::local --> B[Local Burst Duty Cycle<br/>On-time within one MFV burst]:::local
    B --> C[Local MFV Thermal and Cavitation Risk]:::guard

    D[MFV Dwell Time<br/>plus revisit interval]:::global --> E[Global Session Duty Cycle<br/>Aggregate aperture loading across PTV]:::global
    E --> F[Skull and near-field thermal budget]:::guard

    C --> G[Driver scales output<br/>or inserts hold time]:::guard
    F --> G
    G --> H[Advance when both local and global limits are clear]:::global"#;

        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("#rect"));
        assert!(rendered.contains("Local Burst Duty Cycle"));
    }

    #[test]
    fn renders_prd_dual_mode_spatial_flowchart() {
        let src = r#"flowchart LR
    classDef pWave fill:#e1f5fe,stroke:#0288d1,stroke-width:2px;
    classDef fWave fill:#f3e5f5,stroke:#7b1fa2,stroke-width:2px;
    classDef note fill:#fff7e8,stroke:#b26b00,stroke-width:1px;

    TR[Phased Array Transducer Matrix]
    
    subgraph Spatial_Depth_Gradient
        PER[Peripheral Brain Cortex under 40 mm]:::pWave
        DEP[Deep Brain Structures over 40 mm]:::fWave
    end

    TR -->|Mode A: Defocused Broad Beam| PER
    TR -->|Mode B: Focal Constructive Interference| DEP

    PER -.->|Uniform Global Drive Power| M1(Shallow Volumetric Coverage)
    PER -.->|Global Temporal Shift-Keying| M1
    DEP -.->|Intense Phase-Delay Inversion| M2(Deep Constrained MFV)
    DEP -.->|High Angular Convergence| M2
    M1 --- N1[Peripheral coverage favors broad field uniformity<br/>over steep cone angles to prevent bone heating]:::note
    M2 --- N2[Deep targets favor sharp constructive overlap<br/>locked strictly to the precise phase correction matrix]:::note"#;

        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("#rect"));
        assert!(rendered.contains("Phased Array Transducer Matrix"));
    }

    #[test]
    fn renders_flowchart_subgraph_container() {
        let src = r#"flowchart LR
    subgraph Planning_and_Logistics
        NAV[Neuronavigation System]
        IMG[CT and MRI Pre-op Data]
    end
    NAV --> IMG"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("Planning_and_Logistics"));
        assert!(rendered.contains("Neuronavigation System"));
    }

    #[test]
    fn renders_nested_flowchart_subgraph_containers() {
        let src = r#"flowchart LR
    subgraph Outer
        subgraph Inner
            A[Alpha]
        end
        B[Beta]
    end
    A --> B"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("Outer"));
        assert!(rendered.contains("Inner"));
        assert!(rendered.contains("Alpha"));
    }

    #[test]
    fn renders_flowchart_edge_polyline() {
        let src = r#"flowchart LR
    A[Alpha] --> B[Beta]
    A --> C[Gamma]
    C --> D[Delta]"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("#path(fill: none, stroke:"));
        assert!(rendered.contains("), ("));
    }

    #[test]
    fn distinguishes_line_from_arrowhead() {
        let src = r#"flowchart LR
    A[Alpha] --- B[Beta]
    B --> C[Gamma]"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("#path(fill: rgb(\"#555555\"), stroke: 0.6pt + rgb(\"#555555\"), closed: true"));
        assert!(!rendered.contains("[#text(size: 14pt)[▼]]"));
    }

    #[test]
    fn renders_prd_sequence_diagram() {
        let src = r#"sequenceDiagram
    participant PLAN as Delivery Planner
    participant ED as Electronic Driver
    participant TR as Therapy Transducer
    participant PTV as Target Tissue
    participant PAM as Passive Acoustic Mapping
    participant SFE as Safety Feedback Engine

    PLAN->>ED: Load next MFV recipe
    Note over PLAN,SFE: 1 MFV is treated by a burst of 10 to 20 pulses
    loop Each pulse in active MFV burst
        ED->>TR: Transmit pulse 10 us to 4 ms
        TR->>PTV: Emit Acoustic Wave
        PAM->>SFE: Evaluate subharmonic and broadband content
        alt Cavitation Detected
            SFE->>ED: Reduce output or abort before next pulse
        else Safe Threshold Maintained
            SFE-->>ED: Permit next pulse
        end
    end"#;

        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("Delivery Planner"));
        assert!(rendered.contains("Loop: Each pulse in active MFV burst"));
        assert!(rendered.contains("Permit next pulse"));
    }

    #[test]
    fn renders_sequence_activation_blocks() {
        let src = r#"sequenceDiagram
    participant UI
    participant API
    participant DSP

    UI->>API: Start sonication
    activate API
    par Parallel safety checks
        API->>DSP: Load waveform
        activate DSP
        DSP-->>API: Ready
        deactivate DSP
    and Telemetry
        API-->>UI: Progress update
    end
    deactivate API"#;

        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("activate API"));
        assert!(rendered.contains("Par: Parallel safety checks"));
        assert!(rendered.contains("deactivate DSP"));
        assert!(rendered.contains("#rect(width: 14pt"));
    }

    #[test]
    fn renders_basic_gantt_diagram() {
        let src = r#"gantt
    title Sonication Timing Definitions
    dateFormat  X
    axisFormat %L

    section Pulse-Level View
    Pulse 1 active        :p1, 0, 12
    Pulse interval        :pi1, 12, 8
    Pulse 2 active        :p2, 20, 12

    section Burst-Level View
    Burst 1 duration      :b1, 0, 60
    Burst interval        :bi1, 60, 35"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("Sonication Timing Definitions"));
        assert!(rendered.contains("Pulse 1 active"));
        assert!(rendered.contains("#rect"));
    }

    #[test]
    fn renders_basic_state_diagram() {
        let src = r#"stateDiagram-v2
    [*] --> Idle
    Idle --> Armed: plan loaded
    Armed --> Sonicating: start
    Sonicating --> Fault: threshold exceeded
    Fault --> [*]"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("Idle"));
        assert!(rendered.contains("threshold exceeded"));
        assert!(rendered.contains("#rect"));
    }

    #[test]
    fn renders_basic_class_diagram() {
        let src = r#"classDiagram
    class TherapyController {
      +start()
      +abort()
    }
    class SafetyEngine {
      +monitor()
    }
    TherapyController --> SafetyEngine : supervises"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("TherapyController"));
        assert!(rendered.contains("monitor()"));
        assert!(rendered.contains("supervises"));
    }

    #[test]
    fn renders_basic_er_diagram() {
        let src = r#"erDiagram
    PATIENT ||--o{ TREATMENT : receives
    PATIENT {
      string id
      string name
    }
    TREATMENT {
      string modality
    }"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("PATIENT"));
        assert!(rendered.contains("modality"));
        assert!(rendered.contains("receives"));
    }

    #[test]
    fn renders_basic_pie_diagram() {
        let src = r#"pie
    title Treatment Allocation
    "Shallow" : 35
    "Deep" : 65"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("Treatment Allocation"));
        assert!(rendered.contains("Shallow"));
    }

    #[test]
    fn renders_basic_timeline_diagram() {
        let src = r#"timeline
    title Treatment Path
    Planning : Imaging and targeting complete
    Delivery : Sonication begins
    Review : Post-treatment summary exported"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("Treatment Path"));
        assert!(rendered.contains("Planning"));
        assert!(rendered.contains("Sonication begins"));
    }

    #[test]
    fn renders_basic_journey_diagram() {
        let src = r#"journey
    title Clinical Journey
    Imaging Review: 4: Clinician
    Target Planning: 5: Clinician, Physicist
    Sonication Monitoring: 3: Clinician"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("Clinical Journey"));
        assert!(rendered.contains("Target Planning"));
        assert!(rendered.contains("Physicist"));
    }

    #[test]
    fn renders_basic_mindmap_diagram() {
        let src = r#"mindmap
  Treatment Platform
    Planning
      Imaging
      Targeting
    Delivery
      Sonication
      Safety
    Review
      Export"#;
        let rendered = mermaid_to_typst(src).unwrap();
        assert!(rendered.contains("Treatment Platform"));
        assert!(rendered.contains("Sonication"));
        assert!(rendered.contains("#rect"));
    }
}
