use std::io::Cursor;
use std::path::{Path, PathBuf};
use anyhow::Result;
use pulldown_cmark::{Event, CodeBlockKind, Tag, TagEnd};
use docx_rs::*;
use crate::translation::elements::{handle_event, TextState};
use crate::translation::math::latex_to_omml::{LatexToOmmlTranslator, OmmlRenderer};
use crate::translation::mermaid::mermaid_to_drawingml::DrawingMlAstGenerator;
use marksmen_mermaid::parsing::parser;
use marksmen_mermaid::graph::directed_graph;
use marksmen_mermaid::layout::{rank_assignment, crossing_reduction, coordinate_assign};
use marksmen_core::Config;

pub fn convert(events: Vec<Event<'_>>, config: &Config, input_dir: &Path) -> Result<Vec<u8>> {
    let page_width_twips = parse_length_to_twips(&config.page.width).unwrap_or(11906);
    let page_height_twips = parse_length_to_twips(&config.page.height).unwrap_or(16838);
    let margin_top_twips = parse_length_to_twips(&config.page.margin_top).unwrap_or(1701);
    let margin_right_twips = parse_length_to_twips(&config.page.margin_right).unwrap_or(1417);
    let margin_bottom_twips = parse_length_to_twips(&config.page.margin_bottom).unwrap_or(1701);
    let margin_left_twips = parse_length_to_twips(&config.page.margin_left).unwrap_or(1417);
    let mut doc = Docx::new()
        .page_size(page_width_twips, page_height_twips)
        .page_margin(
            PageMargin::new()
                .top(margin_top_twips as i32)
                .right(margin_right_twips as i32)
                .bottom(margin_bottom_twips as i32)
                .left(margin_left_twips as i32)
        )
        // Default Typography: Helvetica/Arial 11pt, resolving the overlap crash
        .default_fonts(RunFonts::new().ascii("Arial").hi_ansi("Arial").cs("Arial").east_asia("Arial"))
        .default_size(22) // 22 half-points = 11pt
        .add_style(Style::new("Heading1", StyleType::Paragraph).name("heading 1").size(48).bold()) // 24pt
        .add_style(Style::new("Heading2", StyleType::Paragraph).name("heading 2").size(36).bold()) // 18pt
        .add_style(Style::new("Heading3", StyleType::Paragraph).name("heading 3").size(28).bold()) // 14pt
        .add_style(Style::new("Heading4", StyleType::Paragraph).name("heading 4").size(24).bold()) // 12pt
        .add_style(Style::new("Heading5", StyleType::Paragraph).name("heading 5").size(22).bold()) // 11pt
        .add_style(Style::new("Heading6", StyleType::Paragraph).name("heading 6").size(20).bold()); // 10pt

    // Inject Title Page
    if !config.title.is_empty() {
        doc = doc.add_paragraph(
            Paragraph::new()
                .align(AlignmentType::Center)
                .add_run(Run::new().size(48).bold().add_text(&config.title))
        );
        doc = doc.add_paragraph(Paragraph::new()); // blank line
    }
    
    if !config.author.is_empty() {
        doc = doc.add_paragraph(
            Paragraph::new()
                .align(AlignmentType::Center)
                .add_run(Run::new().size(28).add_text(&config.author))
        );
    }
    
    if !config.date.is_empty() {
        doc = doc.add_paragraph(
            Paragraph::new()
                .align(AlignmentType::Center)
                .add_run(Run::new().size(22).add_text(&config.date))
        );
        doc = doc.add_paragraph(Paragraph::new()); // blank line
    }
    
    // Page Break after Title Page to physically drop onto Page 2
    if !config.title.is_empty() {
        doc = doc.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_break(BreakType::Page))
        );
    }

    let mut current_paragraph = Paragraph::new();
    let mut text_state = TextState::default();
    let omml = LatexToOmmlTranslator::new();
    
    let mut in_mermaid_block = false;
    let mut current_mermaid_source = String::new();
    let mut in_blockquote = false;
    let (max_figure_width_px, max_figure_height_px) = figure_bounds_px(
        page_width_twips,
        page_height_twips,
        margin_left_twips,
        margin_right_twips,
        margin_top_twips,
        margin_bottom_twips,
    );
    
    // Process markdown token stream into DOCX elements
    let mut event_iter = events.into_iter();
    while let Some(event) = event_iter.next() {
        match event {
            Event::Start(Tag::Table(aligns)) => {
                // Flush preceding paragraph
                if text_state.has_runs {
                    let prev_p = std::mem::replace(&mut current_paragraph, Paragraph::new());
                    doc = doc.add_paragraph(prev_p);
                    text_state.has_runs = false;
                }

                let mut rows = Vec::new();
                let mut current_cells = Vec::new();
                let mut current_cell_p = Paragraph::new();
                let mut cell_index = 0;

                while let Some(te) = event_iter.next() {
                    match te {
                        Event::End(TagEnd::Table) => break,
                        Event::Start(Tag::TableRow) | Event::Start(Tag::TableHead) => {
                            current_cells.clear();
                            cell_index = 0;
                        }
                        Event::End(TagEnd::TableRow) | Event::End(TagEnd::TableHead) => {
                            rows.push(TableRow::new(current_cells.clone()));
                        }
                        Event::Start(Tag::TableCell) => {
                            current_cell_p = Paragraph::new();
                            if cell_index < aligns.len() {
                                match aligns[cell_index] {
                                    pulldown_cmark::Alignment::Center => {
                                        current_cell_p = current_cell_p.align(AlignmentType::Center);
                                    }
                                    pulldown_cmark::Alignment::Right => {
                                        current_cell_p = current_cell_p.align(AlignmentType::Right);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Event::End(TagEnd::TableCell) => {
                            current_cells.push(
                                TableCell::new()
                                    .add_paragraph(current_cell_p.clone())
                            );
                            cell_index += 1;
                        }
                        _ => handle_event(te, &mut doc, &mut current_cell_p, &mut text_state, in_blockquote),
                    }
                }
                
                let table = Table::new(rows.clone()).layout(TableLayoutType::Autofit);
                println!("DOCX ASSEMBLED TABLE WITH ROWS: {}", rows.len());
                doc = doc.add_table(table);
                continue;
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang))) if lang.as_ref() == "mermaid" => {
                in_mermaid_block = true;
                current_mermaid_source.clear();
                continue;
            }
            Event::Text(ref text) if in_mermaid_block => {
                current_mermaid_source.push_str(text.as_ref());
                continue;
            }
            Event::End(TagEnd::CodeBlock) if in_mermaid_block => {
                in_mermaid_block = false;
                
                // Parse, calculate graph topology, and solve coordinates
                let ast = match parser::parse(&current_mermaid_source) {
                    Ok(a) => a,
                    Err(_) => {
                        // Skip corrupted blocks
                        doc = doc.add_paragraph(
                            Paragraph::new().add_run(Run::new().add_text(&current_mermaid_source))
                        );
                        continue;
                    }
                };
                
                let directed_graph = directed_graph::ast_to_graph(ast);
                let mut ranked_graph = rank_assignment::assign_ranks(&directed_graph);
                crossing_reduction::minimize_crossings(&mut ranked_graph);
                let spaced_graph = coordinate_assign::assign_coordinates(&ranked_graph);
                
                // Render the SpacedGraph into an SVG string
                let svg_str = render_graph_to_svg(&spaced_graph);
                
                // Flush preceding paragraph
                if text_state.has_runs {
                    let prev_p = std::mem::replace(&mut current_paragraph, Paragraph::new());
                    doc = doc.add_paragraph(prev_p);
                    text_state.has_runs = false;
                }
                
                // Rasterize SVG→PNG and embed as Pic
                if let Some((png_bytes, width, height)) = svg_to_png(svg_str.as_bytes()) {
                    let (width, height) = fit_image_to_bounds(width, height, max_figure_width_px, max_figure_height_px);
                    let pic = Pic::new_with_dimensions(png_bytes, width, height);
                    let run = Run::new().add_image(pic);
                    doc = doc.add_paragraph(
                        Paragraph::new()
                            .align(AlignmentType::Center)
                            .add_run(run)
                    );
                    
                    // Inject metadata invisibly so `marksmen-docx-read` mathematically restores the AST!
                    let mut meta_run = Run::new().vanish().add_text("```mermaid").add_break(BreakType::TextWrapping);
                    for line in current_mermaid_source.lines() {
                        meta_run = meta_run.add_text(line).add_break(BreakType::TextWrapping);
                    }
                    meta_run = meta_run.add_text("```");
                    doc = doc.add_paragraph(Paragraph::new().add_run(meta_run));
                } else {
                    // Fallback: emit the raw mermaid source as text
                    let run = Run::new().fonts(RunFonts::new().ascii("Consolas"))
                        .add_text(format!("```mermaid\n{}\n```", &current_mermaid_source));
                    doc = doc.add_paragraph(Paragraph::new().add_run(run));
                }
                continue;
            }
            Event::InlineMath(latex) => {
                // Render LaTeX as visible italic text (OMML CustomItem is not reliably rendered)
                let run = Run::new()
                    .italic()
                    .fonts(RunFonts::new().ascii("Cambria Math").hi_ansi("Cambria Math"))
                    .add_text(format!(" {} ", &latex));
                current_paragraph = current_paragraph.add_run(run);
                continue; 
            }
            Event::DisplayMath(latex) => {
                // Flush current paragraph
                if text_state.has_runs {
                    let prev_p = std::mem::replace(&mut current_paragraph, Paragraph::new());
                    doc = doc.add_paragraph(prev_p);
                    text_state.has_runs = false;
                }
                // Render display math as centered italic paragraph
                let run = Run::new()
                    .italic()
                    .fonts(RunFonts::new().ascii("Cambria Math").hi_ansi("Cambria Math"))
                    .add_text(latex.to_string());
                doc = doc.add_paragraph(
                    Paragraph::new()
                        .align(AlignmentType::Center)
                        .add_run(run)
                );
                continue;
            }
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                // Consume all inline events up to End(Image), collecting alt text.
                // This prevents alt-text Text events from leaking into the paragraph stream.
                let mut alt_text = String::new();
                loop {
                    match event_iter.next() {
                        Some(Event::End(TagEnd::Image)) | None => break,
                        Some(Event::Text(t)) => alt_text.push_str(t.as_ref()),
                        _ => {}
                    }
                }
                // Prefer the markdown title field; fall back to collected alt text.
                let caption = if !title.is_empty() {
                    title.to_string()
                } else {
                    alt_text
                };

                // Flush current paragraph
                if text_state.has_runs {
                    let prev_p = std::mem::replace(&mut current_paragraph, Paragraph::new());
                    doc = doc.add_paragraph(prev_p);
                    text_state.has_runs = false;
                }

                let img_path_str = dest_url.as_ref();
                let resolved = if Path::new(img_path_str).is_absolute() {
                    PathBuf::from(img_path_str)
                } else {
                    input_dir.join(img_path_str)
                };

                if let Ok(raw_bytes) = std::fs::read(&resolved) {
                    let is_svg = img_path_str.ends_with(".svg")
                        || raw_bytes.starts_with(b"<?xml")
                        || raw_bytes.starts_with(b"<svg");

                    let (png_bytes, width, height) = if is_svg {
                        // SVG → PNG rasterization
                        match svg_to_png(&raw_bytes) {
                            Some(result) => result,
                            None => {
                                // Fallback: emit alt text as placeholder
                                let run = Run::new().add_text(format!("![{}]({})", caption, img_path_str));
                                doc = doc.add_paragraph(Paragraph::new().add_run(run));
                                continue;
                            }
                        }
                    } else {
                        // PNG/JPEG: detect dimensions from header
                        let (w, h) = image_dimensions(&raw_bytes).unwrap_or((640, 480));
                        (raw_bytes, w, h)
                    };

                    let (width, height) = fit_image_to_bounds(width, height, max_figure_width_px, max_figure_height_px);
                    let pic = Pic::new_with_dimensions(png_bytes, width, height);
                    let run = Run::new().add_image(pic);
                    doc = doc.add_paragraph(
                        Paragraph::new()
                            .align(AlignmentType::Center)
                            .add_run(run)
                    );
                } else {
                    // File not found: emit placeholder
                    let run = Run::new().italic().add_text(format!("[Missing image: {}]", img_path_str));
                    doc = doc.add_paragraph(Paragraph::new().add_run(run));
                }
                continue;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                in_blockquote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                in_blockquote = false;
            }
            _ => {
                handle_event(event, &mut doc, &mut current_paragraph, &mut text_state, in_blockquote);
            }
        }
    }

    // Flush final paragraph if pending (and non-empty)
    if text_state.has_runs {
        doc = doc.add_paragraph(current_paragraph);
    }

    // Write to memory buffer
    let mut buffer = Cursor::new(Vec::new());
    doc.build().pack(&mut buffer)?;

    Ok(buffer.into_inner())
}

/// Rasterizes an SVG byte buffer to a PNG byte buffer.
/// Returns (png_bytes, width, height) or None on failure.
fn svg_to_png(svg_data: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_data, &opt).ok()?;
    let size = tree.size();
    let width = size.width() as u32;
    let height = size.height() as u32;
    if width == 0 || height == 0 { return None; }
    
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    let png_data = pixmap.encode_png().ok()?;
    Some((png_data, width, height))
}

/// Extracts image dimensions from raw PNG/JPEG bytes.
fn image_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    // PNG: width at bytes 16..20, height at 20..24 (big-endian)
    if data.len() > 24 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Some((w, h));
    }
    // JPEG: scan for SOF0 marker (0xFF 0xC0)
    if data.len() > 2 && data[0] == 0xFF && data[1] == 0xD8 {
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] == 0xFF && (data[i + 1] == 0xC0 || data[i + 1] == 0xC2) {
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return Some((w, h));
            }
            let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            i += 2 + seg_len;
        }
    }
    None
}

/// Renders a `SpacedGraph` into a valid SVG string for rasterization.
/// Nodes are drawn as rounded rectangles with centered text labels.
/// Edges are drawn as straight lines with arrowhead markers.
fn render_graph_to_svg(graph: &marksmen_mermaid::layout::coordinate_assign::SpacedGraph) -> String {
    let padding = 20.0;
    let svg_width = graph.width + padding * 2.0;
    let svg_height = graph.height + padding * 2.0;

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
  <rect width="{w}" height="{h}" fill="white"/>"##,
        w = svg_width, h = svg_height
    );

    let mut ordered_subgraphs = graph.subgraphs.clone();
    ordered_subgraphs.sort_by(|left, right| left.depth.cmp(&right.depth).then_with(|| left.title.cmp(&right.title)));
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
            xml_escape(&subgraph.title)
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
            let stroke_path = if edge.style == marksmen_mermaid::parsing::lexer::EdgeStyle::SolidLine {
                edge.path.clone()
            } else {
                trim_path_end(&edge.path, 10.0)
            };
            let points = stroke_path.iter()
                .map(|(x, y)| format!("{},{}", x + padding, y + padding))
                .collect::<Vec<_>>()
                .join(" ");
            svg.push_str(&format!(
                r##"  <polyline points="{}" fill="none" stroke="#555" stroke-width="{}"{} stroke-linejoin="round" stroke-linecap="round" />"##,
                points,
                stroke_width,
                dash,
            ));
            svg.push('\n');

            if edge.style != marksmen_mermaid::parsing::lexer::EdgeStyle::SolidLine {
                if let Some([tip, left, right]) = arrowhead_points(&edge.path, 10.0, 7.0) {
                    svg.push_str(&format!(
                        r##"  <polygon points="{},{} {},{} {},{}" fill="#555" stroke="#555" stroke-width="0.6" />"##,
                        tip.0 + padding,
                        tip.1 + padding,
                        left.0 + padding,
                        left.1 + padding,
                        right.0 + padding,
                        right.1 + padding
                    ));
                    svg.push('\n');
                }
            }

            if let Some(label) = &edge.label {
                let (label_x, label_y) = edge_label_anchor(&edge.path);
                let text_x = label_x + padding;
                let text_y = label_y + padding - 6.0;
                svg.push_str(&format!(
                    r##"  <rect x="{}" y="{}" width="{}" height="18" fill="white" opacity="0.9"/>"##,
                    text_x - 70.0,
                    text_y - 12.0,
                    140.0
                ));
                svg.push('\n');
                svg.push_str(&format!(
                    r##"  <text x="{}" y="{}" text-anchor="middle" font-family="Arial, sans-serif" font-size="11" fill="#444">{}</text>"##,
                    text_x,
                    text_y,
                    xml_escape(label)
                ));
                svg.push('\n');
            }
        }
    }

    // Draw nodes
    for (_id, node) in &graph.nodes {
        let rx = node.x + padding;
        let ry = node.y + padding;
        let fill = node.style.fill.as_deref().unwrap_or("#E8F4FD");
        let stroke = node.style.stroke.as_deref().unwrap_or("#2196F3");
        let stroke_width = node.style.stroke_width.as_deref().unwrap_or("2");
        let text_fill = node.style.color.as_deref().unwrap_or("#333");
        let dash = node.style.stroke_dasharray.as_deref()
            .map(|v| format!(r#" stroke-dasharray="{}""#, v))
            .unwrap_or_default();
        svg.push_str(&format!(
            r##"  <rect x="{}" y="{}" width="{}" height="{}" rx="6" ry="6" fill="{}" stroke="{}" stroke-width="{}"{} />"##,
            rx, ry, node.width, node.height, fill, stroke, stroke_width, dash
        ));
        svg.push('\n');
        // Center text label
        let text_x = rx + node.width / 2.0;
        let text_y = ry + node.height / 2.0 + 5.0;
        svg.push_str(&format!(
            r##"  <text x="{}" y="{}" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="{}">{}</text>"##,
            text_x, text_y, text_fill, xml_escape(&node.label)
        ));
        svg.push('\n');
    }

    svg.push_str("</svg>");
    svg
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

fn parse_length_to_twips(input: &str) -> Option<u32> {
    let trimmed = input.trim().to_ascii_lowercase();
    let (value, unit) = trimmed
        .chars()
        .position(|ch| !(ch.is_ascii_digit() || ch == '.'))
        .map(|idx| trimmed.split_at(idx))?;
    let value = value.trim().parse::<f64>().ok()?;
    let twips = match unit.trim() {
        "in" => value * 1440.0,
        "pt" => value * 20.0,
        "cm" => (value / 2.54) * 1440.0,
        "mm" => (value / 25.4) * 1440.0,
        "twip" | "twips" => value,
        _ => return None,
    };
    Some(twips.round().max(1.0) as u32)
}

fn figure_bounds_px(
    page_width_twips: u32,
    page_height_twips: u32,
    margin_left_twips: u32,
    margin_right_twips: u32,
    margin_top_twips: u32,
    margin_bottom_twips: u32,
) -> (u32, u32) {
    let content_width_twips = page_width_twips.saturating_sub(margin_left_twips + margin_right_twips);
    let content_height_twips = page_height_twips.saturating_sub(margin_top_twips + margin_bottom_twips);

    let max_width_px = twips_to_px(content_width_twips).max(320);
    // Keep figures visually aligned with PDF output and avoid page overflow in Word.
    let max_height_px = ((twips_to_px(content_height_twips) as f64) * 0.72).round() as u32;
    (max_width_px, max_height_px.max(240))
}

fn twips_to_px(twips: u32) -> u32 {
    (((twips as f64) / 1440.0) * 96.0).round() as u32
}

fn fit_image_to_bounds(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (width.max(1), height.max(1));
    }

    let width_ratio = max_width as f64 / width as f64;
    let height_ratio = max_height as f64 / height as f64;
    let scale = width_ratio.min(height_ratio).min(1.0);

    (
        ((width as f64) * scale).round().max(1.0) as u32,
        ((height as f64) * scale).round().max(1.0) as u32,
    )
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
