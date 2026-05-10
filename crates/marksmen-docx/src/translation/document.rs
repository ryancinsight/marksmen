use crate::translation::elements::{handle_event, Container, TextState};
use anyhow::Result;
use docx_rs::*;
use marksmen_core::Config;
use marksmen_mermaid::graph::directed_graph;
use marksmen_mermaid::layout::{coordinate_assign, crossing_reduction, rank_assignment};
use marksmen_mermaid::parsing::parser;
use marksmen_render::{render_math_to_png, render_mmd_to_png, svg_bytes_to_png};
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use std::io::Cursor;
use std::path::{Path, PathBuf};

fn load_references(
    input_dir: &Path,
) -> std::collections::HashMap<String, marksmen_csl::model::Reference> {
    let mut map = std::collections::HashMap::new();
    let mut try_parse = |path: std::path::PathBuf| {
        if let Ok(json) = std::fs::read_to_string(&path) {
            if let Ok(refs) = serde_json::from_str::<Vec<marksmen_csl::model::Reference>>(&json) {
                for r in refs {
                    map.insert(r.id.clone(), r);
                }
            }
        }
    };

    try_parse(input_dir.join("references.json"));
    if let Ok(appdata) = std::env::var("APPDATA") {
        try_parse(Path::new(&appdata).join("marksmen").join("references.json"));
    }
    map
}

fn format_reference(r: &marksmen_csl::model::Reference) -> String {
    let mut s = String::new();
    if let Some(authors) = &r.author {
        let mut names = Vec::new();
        for a in authors {
            let mut name = String::new();
            if let Some(first) = &a.given {
                name.push_str(first);
                name.push(' ');
            }
            if let Some(last) = &a.family {
                name.push_str(last);
            }
            if !name.is_empty() {
                names.push(name.trim().to_string());
            }
        }
        if !names.is_empty() {
            s.push_str(&names.join(", "));
            s.push_str(". ");
        }
    }
    if let Some(title) = &r.title {
        s.push_str(title);
        s.push_str(". ");
    }
    if let Some(container) = &r.container_title {
        s.push_str(container);
        s.push_str(", ");
    }
    if let Some(vol) = &r.volume {
        s.push_str(&format!("vol. {}, ", vol));
    }
    if let Some(page) = &r.page {
        s.push_str(&format!("pp. {}, ", page));
    }
    if let Some(issued) = &r.issued {
        let dp = &issued.date_parts;
        if let Some(year) = dp.get(0).and_then(|a| a.get(0)) {
            s.push_str(&format!("{}. ", year));
        }
    }
    s.trim().to_string()
}

pub fn convert(
    events: &[Event<'_>],
    config: &Config,
    input_dir: &Path,
    source_docx: Option<&[u8]>,
) -> Result<Vec<u8>> {
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
                .left(margin_left_twips as i32),
        )
        // Default Typography: Helvetica/Arial 11pt, resolving the overlap crash
        .default_fonts(
            RunFonts::new()
                .ascii("Arial")
                .hi_ansi("Arial")
                .cs("Arial")
                .east_asia("Arial"),
        )
        .default_size(22) // 22 half-points = 11pt
        .add_style(
            Style::new("Heading1", StyleType::Paragraph)
                .name("heading 1")
                .size(48)
                .bold(),
        ) // 24pt
        .add_style(
            Style::new("Heading2", StyleType::Paragraph)
                .name("heading 2")
                .size(36)
                .bold(),
        ) // 18pt
        .add_style(
            Style::new("Heading3", StyleType::Paragraph)
                .name("heading 3")
                .size(28)
                .bold(),
        ) // 14pt
        .add_style(
            Style::new("Heading4", StyleType::Paragraph)
                .name("heading 4")
                .size(24)
                .bold(),
        ) // 12pt
        .add_style(
            Style::new("Heading5", StyleType::Paragraph)
                .name("heading 5")
                .size(22)
                .bold(),
        ) // 11pt
        .add_style(
            Style::new("Heading6", StyleType::Paragraph)
                .name("heading 6")
                .size(20)
                .bold(),
        ) // 10pt
        .add_style(Style::new("CodeBlock", StyleType::Paragraph).name("CodeBlock"))
        // Bullet list: numbering-id 1
        .add_abstract_numbering(
            AbstractNumbering::new(1)
                .add_level(
                    Level::new(
                        0,
                        Start::new(1),
                        NumberFormat::new("bullet"),
                        LevelText::new("\u{2022}"),
                        LevelJc::new("left"),
                    )
                    .indent(
                        Some(720),
                        Some(SpecialIndentType::Hanging(360)),
                        None,
                        None,
                    ),
                )
                .add_level(
                    Level::new(
                        1,
                        Start::new(1),
                        NumberFormat::new("bullet"),
                        LevelText::new("\u{25E6}"),
                        LevelJc::new("left"),
                    )
                    .indent(
                        Some(1440),
                        Some(SpecialIndentType::Hanging(360)),
                        None,
                        None,
                    ),
                )
                .add_level(
                    Level::new(
                        2,
                        Start::new(1),
                        NumberFormat::new("bullet"),
                        LevelText::new("\u{25AA}"),
                        LevelJc::new("left"),
                    )
                    .indent(
                        Some(2160),
                        Some(SpecialIndentType::Hanging(360)),
                        None,
                        None,
                    ),
                ),
        )
        .add_numbering(Numbering::new(1, 1))
        // Decimal list: numbering-id 2
        .add_abstract_numbering(
            AbstractNumbering::new(2)
                .add_level(
                    Level::new(
                        0,
                        Start::new(1),
                        NumberFormat::new("decimal"),
                        LevelText::new("%1."),
                        LevelJc::new("left"),
                    )
                    .indent(
                        Some(720),
                        Some(SpecialIndentType::Hanging(360)),
                        None,
                        None,
                    ),
                )
                .add_level(
                    Level::new(
                        1,
                        Start::new(1),
                        NumberFormat::new("decimal"),
                        LevelText::new("%2."),
                        LevelJc::new("left"),
                    )
                    .indent(
                        Some(1440),
                        Some(SpecialIndentType::Hanging(360)),
                        None,
                        None,
                    ),
                )
                .add_level(
                    Level::new(
                        2,
                        Start::new(1),
                        NumberFormat::new("decimal"),
                        LevelText::new("%3."),
                        LevelJc::new("left"),
                    )
                    .indent(
                        Some(2160),
                        Some(SpecialIndentType::Hanging(360)),
                        None,
                        None,
                    ),
                ),
        )
        .add_numbering(Numbering::new(2, 2));

    // Inject Title Page
    if !config.title.is_empty() {
        doc = doc.add_paragraph(
            Paragraph::new()
                .align(AlignmentType::Center)
                .add_run(Run::new().size(48).bold().add_text(&config.title)),
        );
        doc = doc.add_paragraph(Paragraph::new()); // blank line
    }

    if !config.author.is_empty() {
        doc = doc.add_paragraph(
            Paragraph::new()
                .align(AlignmentType::Center)
                .add_run(Run::new().size(28).add_text(&config.author)),
        );
    }

    if !config.date.is_empty() {
        doc = doc.add_paragraph(
            Paragraph::new()
                .align(AlignmentType::Center)
                .add_run(Run::new().size(22).add_text(&config.date)),
        );
        doc = doc.add_paragraph(Paragraph::new()); // blank line
    }

    // Page Break after Title Page to physically drop onto Page 2
    if !config.title.is_empty() {
        doc = doc.add_paragraph(Paragraph::new().add_run(Run::new().add_break(BreakType::Page)));
    }

    let mut current_paragraph = Paragraph::new();
    let mut text_state = TextState::default();

    // Pre-populate the source comment ID lookup table so handle_event uses original
    // IDs from the source DOCX when writing w:commentRangeStart anchors in document.xml.
    // This must be done before the event loop so the first comment mark gets the right ID.
    if let Some(src_bytes) = source_docx {
        if let Ok(mut src_zip) = zip::ZipArchive::new(std::io::Cursor::new(src_bytes)) {
            if let Ok(mut cf) = src_zip.by_name("word/comments.xml") {
                let mut cxml = String::new();
                let _ = std::io::Read::read_to_string(&mut cf, &mut cxml);
                let mut reader = quick_xml::Reader::from_str(&cxml);
                reader.config_mut().trim_text(true);
                let mut buf = Vec::new();

                let mut current_id = None;
                let mut in_comment = false;
                let mut in_t = false;
                let mut current_text = String::new();

                loop {
                    match reader.read_event_into(&mut buf) {
                        Ok(quick_xml::events::Event::Start(ref e))
                            if e.name().as_ref() == b"w:comment" =>
                        {
                            in_comment = true;
                            current_text.clear();
                            current_id = e
                                .attributes()
                                .filter_map(|a| a.ok())
                                .find(|a| a.key.as_ref() == b"w:id")
                                .and_then(|a| String::from_utf8(a.value.into_owned()).ok());
                        }
                        Ok(quick_xml::events::Event::End(ref e))
                            if e.name().as_ref() == b"w:comment" =>
                        {
                            if let Some(id_str) = current_id.take() {
                                let text_norm = current_text.trim().to_ascii_lowercase();
                                if let Ok(id_num) = id_str.parse::<usize>() {
                                    if !text_norm.is_empty() {
                                        text_state.source_comment_ids.insert(text_norm, id_num);
                                    }
                                }
                            }
                            in_comment = false;
                        }
                        Ok(quick_xml::events::Event::Start(ref e))
                            if e.name().as_ref() == b"w:t" && in_comment =>
                        {
                            in_t = true;
                        }
                        Ok(quick_xml::events::Event::End(ref e)) if e.name().as_ref() == b"w:t" => {
                            in_t = false;
                        }
                        Ok(quick_xml::events::Event::Text(ref e)) if in_t => {
                            if let Ok(t) = e.unescape() {
                                current_text.push_str(&t);
                            }
                        }
                        Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                        _ => (),
                    }
                    buf.clear();
                }
            }
        }
    }

    // List state: parallel stacks for depth and ordered/bullet classification.
    // NumberingId 1 = bullet (unordered), NumberingId 2 = decimal (ordered).
    let mut list_ordered_stack: Vec<bool> = Vec::new();

    let mut in_mermaid_block = false;
    let mut current_mermaid_source = String::new();
    let mut in_generic_code_block = false;
    let mut current_generic_code_source = String::new();
    let mut in_blockquote = false;
    let (max_figure_width_px, max_figure_height_px) = figure_bounds_px(
        page_width_twips,
        page_height_twips,
        margin_left_twips,
        margin_right_twips,
        margin_top_twips,
        margin_bottom_twips,
    );

    // Extract Footnotes in first pass before main loop
    let mut footnote_map: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut footnote_counter: usize = 1;
    for event in events.iter().cloned() {
        if let Event::Start(Tag::FootnoteDefinition(label)) = event {
            footnote_map.insert(label.to_string(), footnote_counter);
            footnote_counter += 1;
        }
    }

    // Two-pass footnote resolution:
    // Pass 1 – scan the event slice, collect paragraph content for each
    //          FootnoteDefinition keyed by the numeric id from footnote_map.
    //          This must run before the consuming main loop.
    let mut footnote_content: std::collections::HashMap<usize, Vec<Paragraph>> =
        std::collections::HashMap::new();
    {
        let mut scan_iter = events.iter().peekable();
        while let Some(ev) = scan_iter.next() {
            if let Event::Start(Tag::FootnoteDefinition(label)) = ev {
                let fn_id = footnote_map.get(label.as_ref()).copied().unwrap_or(0);
                if fn_id == 0 {
                    continue;
                }
                let mut fn_paragraphs: Vec<Paragraph> = Vec::new();
                let mut fn_paragraph = Paragraph::new();
                let mut fn_has_runs = false;
                for inner in scan_iter.by_ref() {
                    match inner {
                        Event::End(TagEnd::FootnoteDefinition) => break,
                        Event::End(TagEnd::Paragraph) if fn_has_runs => {
                            fn_paragraphs
                                .push(std::mem::replace(&mut fn_paragraph, Paragraph::new()));
                            fn_has_runs = false;
                        }
                        Event::Text(t) => {
                            fn_paragraph = fn_paragraph.add_run(Run::new().add_text(t.to_string()));
                            fn_has_runs = true;
                        }
                        Event::Code(t) => {
                            fn_paragraph = fn_paragraph.add_run(
                                Run::new()
                                    .fonts(RunFonts::new().ascii("Consolas"))
                                    .add_text(t.to_string()),
                            );
                            fn_has_runs = true;
                        }
                        Event::Start(Tag::Strong)
                        | Event::End(TagEnd::Strong)
                        | Event::Start(Tag::Emphasis)
                        | Event::End(TagEnd::Emphasis) => {}
                        _ => {}
                    }
                }
                if fn_has_runs {
                    fn_paragraphs.push(fn_paragraph);
                }
                footnote_content.insert(fn_id, fn_paragraphs);
            }
        }
    }

    // Process markdown token stream into DOCX elements
    let mut event_iter = events.iter().cloned();
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
                let mut current_cells: Vec<TableCell> = Vec::new();
                let mut current_tc = TableCell::new();
                let mut current_cell_p = Paragraph::new();
                let mut cell_index = 0;

                let mut cell_grid_spans: Vec<usize> = Vec::new();
                let mut current_cell_is_colspan = false;

                while let Some(te) = event_iter.by_ref().next() {
                    match te {
                        Event::End(TagEnd::Table) => break,
                        Event::Start(Tag::TableRow) | Event::Start(Tag::TableHead) => {
                            current_cells.clear();
                            cell_grid_spans.clear();
                            cell_index = 0;
                        }
                        Event::End(TagEnd::TableRow) | Event::End(TagEnd::TableHead) => {
                            for (cell, span) in current_cells.iter_mut().zip(cell_grid_spans.iter())
                            {
                                if *span > 1 {
                                    *cell = cell.clone().grid_span(*span);
                                }
                            }
                            rows.push(TableRow::new(std::mem::take(&mut current_cells)));
                        }
                        Event::Start(Tag::TableCell) => {
                            current_tc = TableCell::new();
                            current_cell_p = Paragraph::new();
                            current_cell_is_colspan = false;
                            if cell_index < aligns.len() {
                                match aligns[cell_index] {
                                    pulldown_cmark::Alignment::Center => {
                                        current_cell_p =
                                            current_cell_p.align(AlignmentType::Center);
                                    }
                                    pulldown_cmark::Alignment::Right => {
                                        current_cell_p = current_cell_p.align(AlignmentType::Right);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Event::End(TagEnd::TableCell) => {
                            if !current_cell_is_colspan {
                                current_tc = current_tc.add_paragraph(std::mem::replace(
                                    &mut current_cell_p,
                                    Paragraph::new(),
                                ));
                                current_cells
                                    .push(std::mem::replace(&mut current_tc, TableCell::new()));
                                cell_grid_spans.push(1);
                            }
                            cell_index += 1;
                        }
                        ev @ (Event::Html(_) | Event::InlineHtml(_)) => {
                            let is_nested = match &ev {
                                Event::Html(h) | Event::InlineHtml(h) => {
                                    let tlow = h.to_lowercase();
                                    if tlow.contains("<!-- colspan -->") {
                                        current_cell_is_colspan = true;
                                        if let Some(last_span) = cell_grid_spans.last_mut() {
                                            *last_span += 1;
                                        }
                                        continue;
                                    } else if tlow.contains("<!-- bg_color:") {
                                        let bg = tlow
                                            .split("<!-- bg_color:")
                                            .nth(1)
                                            .unwrap_or("")
                                            .split("-->")
                                            .next()
                                            .unwrap_or("")
                                            .trim()
                                            .to_uppercase();
                                        if !bg.is_empty() {
                                            current_tc = current_tc.shading(
                                                docx_rs::Shading::new()
                                                    .fill(bg)
                                                    .shd_type(docx_rs::ShdType::Clear)
                                                    .color("auto"),
                                            );
                                        }
                                        continue;
                                    } else if tlow.contains("<!-- p_br -->") {
                                        current_tc = current_tc.add_paragraph(std::mem::replace(
                                            &mut current_cell_p,
                                            Paragraph::new(),
                                        ));
                                        continue;
                                    }
                                    tlow.starts_with("<table") && tlow.contains("nested")
                                }
                                _ => false,
                            };

                            if is_nested {
                                let mut n_rows = Vec::new();
                                let mut n_cells = Vec::new();
                                let mut n_tc = TableCell::new();
                                let mut n_p = Paragraph::new();
                                let mut in_td = false;
                                let mut n_is_bold = false;
                                let mut mark_stack = Vec::new();

                                while let Some(ev_n) = event_iter.by_ref().next() {
                                    let mut is_text = false;
                                    let html_str = match &ev_n {
                                        Event::Html(h) | Event::InlineHtml(h) => Some(h.as_ref()),
                                        Event::Text(t) => {
                                            is_text = true;
                                            Some(t.as_ref())
                                        }
                                        Event::SoftBreak | Event::HardBreak => {
                                            n_p = n_p.add_run(
                                                Run::new()
                                                    .add_break(docx_rs::BreakType::TextWrapping),
                                            );
                                            continue;
                                        }
                                        _ => continue,
                                    };
                                    if let Some(s) = html_str {
                                        let seg_low = s.to_lowercase();
                                        if !is_text && seg_low.starts_with("</table") {
                                            break;
                                        } else if !is_text && seg_low.starts_with("<tr") {
                                            n_cells.clear();
                                        } else if !is_text && seg_low.starts_with("</tr") {
                                            n_rows
                                                .push(TableRow::new(std::mem::take(&mut n_cells)));
                                        } else if !is_text && seg_low.starts_with("<td") {
                                            in_td = true;
                                            n_tc = TableCell::new();
                                            n_p = Paragraph::new();
                                        } else if !is_text && seg_low.starts_with("</td") {
                                            in_td = false;
                                            n_cells.push(
                                                std::mem::replace(&mut n_tc, TableCell::new())
                                                    .add_paragraph(std::mem::replace(
                                                        &mut n_p,
                                                        Paragraph::new(),
                                                    )),
                                            );
                                        } else if !is_text && seg_low.starts_with("<!-- bg_color:")
                                        {
                                            let bg = seg_low
                                                .split("<!-- bg_color:")
                                                .nth(1)
                                                .unwrap_or("")
                                                .split("-->")
                                                .next()
                                                .unwrap_or("")
                                                .trim()
                                                .to_uppercase();
                                            if !bg.is_empty() {
                                                n_tc = n_tc.shading(
                                                    docx_rs::Shading::new()
                                                        .fill(bg)
                                                        .shd_type(docx_rs::ShdType::Clear)
                                                        .color("auto"),
                                                );
                                            }
                                        } else if !is_text && seg_low.starts_with("<!-- colspan:") {
                                            let span = seg_low
                                                .split("<!-- colspan:")
                                                .nth(1)
                                                .unwrap_or("")
                                                .split("-->")
                                                .next()
                                                .unwrap_or("")
                                                .trim()
                                                .parse()
                                                .unwrap_or(1);
                                            if span > 1 {
                                                n_tc = n_tc.grid_span(span);
                                            }
                                        } else if !is_text
                                            && seg_low.starts_with("<mark")
                                            && seg_low.contains("comment")
                                        {
                                            mark_stack.push("comment");
                                            let author =
                                                crate::translation::elements::extract_attr(
                                                    s,
                                                    "data-author",
                                                )
                                                .unwrap_or_default();
                                            let content =
                                                crate::translation::elements::extract_attr(
                                                    s,
                                                    "data-content",
                                                )
                                                .unwrap_or_default();
                                            let id = text_state.comment_id_counter;
                                            text_state.comment_id_counter += 1;
                                            text_state.active_comment_id = Some(id);
                                            let comment =
                                                Comment::new(id).author(author).add_paragraph(
                                                    Paragraph::new()
                                                        .add_run(Run::new().add_text(content)),
                                                );
                                            n_p = n_p.add_comment_start(comment);
                                        } else if !is_text
                                            && seg_low.starts_with("<mark")
                                            && seg_low.contains("align-center")
                                        {
                                            mark_stack.push("align");
                                            n_p = n_p.align(AlignmentType::Center);
                                        } else if !is_text
                                            && seg_low.starts_with("<div")
                                            && (seg_low.contains("align=\"center\"")
                                                || seg_low.contains("text-align: center")
                                                || seg_low.contains("text-align:center"))
                                        {
                                            n_p = n_p.align(AlignmentType::Center);
                                        } else if !is_text && seg_low.starts_with("</mark") {
                                            if let Some(m) = mark_stack.pop() {
                                                if m == "comment" {
                                                    if let Some(id) =
                                                        text_state.active_comment_id.take()
                                                    {
                                                        n_p = n_p.add_comment_end(id);
                                                    }
                                                }
                                            }
                                        } else if !is_text && seg_low.starts_with("<strong") {
                                            n_is_bold = true;
                                        } else if !is_text && seg_low.starts_with("</strong") {
                                            n_is_bold = false;
                                        } else if !is_text && seg_low.starts_with("<u") {
                                            text_state.is_underline = true;
                                        } else if !is_text && seg_low.starts_with("</u") {
                                            text_state.is_underline = false;
                                        } else if !is_text && seg_low.starts_with("<ins") {
                                            text_state.is_ins = true;
                                            text_state.revision_ins_author =
                                                crate::translation::elements::extract_attr(
                                                    s,
                                                    "data-author",
                                                );
                                            text_state.revision_ins_date =
                                                crate::translation::elements::extract_attr(
                                                    s,
                                                    "data-date",
                                                );
                                        } else if !is_text && seg_low.starts_with("</ins") {
                                            text_state.is_ins = false;
                                        } else if !is_text && seg_low.starts_with("<del") {
                                            text_state.is_del = true;
                                            text_state.revision_del_author =
                                                crate::translation::elements::extract_attr(
                                                    s,
                                                    "data-author",
                                                );
                                            text_state.revision_del_date =
                                                crate::translation::elements::extract_attr(
                                                    s,
                                                    "data-date",
                                                );
                                        } else if !is_text && seg_low.starts_with("</del") {
                                            text_state.is_del = false;
                                        } else if !is_text && seg_low.starts_with("<br") {
                                            n_p = n_p.add_run(
                                                Run::new()
                                                    .add_break(docx_rs::BreakType::TextWrapping),
                                            );
                                        } else if !is_text && seg_low.starts_with("<img") {
                                            let src = crate::translation::elements::extract_attr(
                                                s, "src",
                                            )
                                            .unwrap_or_default();
                                            let alt = crate::translation::elements::extract_attr(
                                                s, "alt",
                                            )
                                            .unwrap_or_default();
                                            let run = resolve_image_to_run(
                                                &src,
                                                &alt,
                                                input_dir,
                                                max_figure_width_px,
                                                max_figure_height_px,
                                            );
                                            n_tc = n_tc.add_paragraph(std::mem::replace(
                                                &mut n_p,
                                                Paragraph::new(),
                                            ));
                                            n_tc = n_tc.add_paragraph(
                                                Paragraph::new()
                                                    .align(AlignmentType::Center)
                                                    .add_run(run),
                                            );
                                        } else if is_text && in_td {
                                            let mut run = Run::new().add_text(s);
                                            if n_is_bold {
                                                run = run.bold();
                                            }
                                            if text_state.is_underline {
                                                run = run.underline("single");
                                            }
                                            if text_state.is_highlight {
                                                run = run.highlight("yellow");
                                            }

                                            if text_state.is_del {
                                                let mut del = docx_rs::Delete::new().add_run(run);
                                                if let Some(author) =
                                                    &text_state.revision_del_author
                                                {
                                                    del = del.author(author);
                                                }
                                                if let Some(date) = &text_state.revision_del_date {
                                                    del = del.date(date);
                                                }
                                                n_p = n_p.add_delete(del);
                                            } else if text_state.is_ins {
                                                let mut ins = docx_rs::Insert::new(run);
                                                if let Some(author) =
                                                    &text_state.revision_ins_author
                                                {
                                                    ins = ins.author(author);
                                                }
                                                if let Some(date) = &text_state.revision_ins_date {
                                                    ins = ins.date(date);
                                                }
                                                n_p = n_p.add_insert(ins);
                                            } else {
                                                n_p = n_p.add_run(run);
                                            }
                                        }
                                    }
                                }
                                let n_cols =
                                    n_rows.first().map(|r| r.cells.len()).unwrap_or(1).max(1);
                                let grid: Vec<usize> = (0..n_cols).map(|_| 9000 / n_cols).collect();
                                let nested_tbl = Table::new(std::mem::take(&mut n_rows))
                                    .layout(TableLayoutType::Autofit)
                                    .width(5000, WidthType::Pct)
                                    .set_grid(grid);
                                current_tc = current_tc.add_paragraph(std::mem::replace(
                                    &mut current_cell_p,
                                    Paragraph::new(),
                                ));
                                current_tc = current_tc.add_table(nested_tbl);
                                current_cell_p = Paragraph::new();
                            } else {
                                handle_event(
                                    ev,
                                    Container::Doc(&mut doc),
                                    &mut current_cell_p,
                                    &mut text_state,
                                    in_blockquote,
                                    config,
                                    None,
                                );
                            }
                        }
                        Event::Start(Tag::Image {
                            dest_url, title, ..
                        }) => {
                            let mut alt_text = String::new();
                            loop {
                                match event_iter.next() {
                                    Some(Event::End(TagEnd::Image)) | None => break,
                                    Some(Event::Text(t)) => alt_text.push_str(t.as_ref()),
                                    _ => {}
                                }
                            }
                            let caption = if !title.is_empty() {
                                title.to_string()
                            } else {
                                alt_text
                            };
                            let run = resolve_image_to_run(
                                dest_url.as_ref(),
                                &caption,
                                input_dir,
                                max_figure_width_px,
                                max_figure_height_px,
                            );
                            current_tc = current_tc.add_paragraph(std::mem::replace(
                                &mut current_cell_p,
                                Paragraph::new(),
                            ));
                            current_tc = current_tc.add_paragraph(
                                Paragraph::new().align(AlignmentType::Center).add_run(run),
                            );
                        }
                        _ => handle_event(
                            te,
                            Container::Doc(&mut doc),
                            &mut current_cell_p,
                            &mut text_state,
                            in_blockquote,
                            config,
                            None,
                        ),
                    }
                }

                let table = Table::new(std::mem::take(&mut rows))
                    .layout(TableLayoutType::Autofit)
                    .width(5000, WidthType::Pct);
                doc = doc.add_table(table);
                continue;
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang))) => {
                if lang.as_ref() == "mermaid" {
                    in_mermaid_block = true;
                    current_mermaid_source.clear();
                } else {
                    in_generic_code_block = true;
                    current_generic_code_source.clear();
                }
                continue;
            }
            Event::Start(Tag::CodeBlock(_)) => {
                in_generic_code_block = true;
                current_generic_code_source.clear();
                continue;
            }
            Event::Text(ref text) if in_mermaid_block => {
                current_mermaid_source.push_str(text.as_ref());
                continue;
            }
            Event::Text(ref text) if in_generic_code_block => {
                current_generic_code_source.push_str(text.as_ref());
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
                            Paragraph::new().add_run(Run::new().add_text(&current_mermaid_source)),
                        );
                        continue;
                    }
                };

                let directed_graph = directed_graph::ast_to_graph(ast);
                let mut ranked_graph = rank_assignment::assign_ranks(&directed_graph);
                crossing_reduction::minimize_crossings(&mut ranked_graph);
                let spaced_graph = coordinate_assign::assign_coordinates(&ranked_graph);

                // Render the SpacedGraph to PNG via marksmen_render
                let png_result = marksmen_render::mermaid::render_graph_to_svg(&spaced_graph);
                let png_result = marksmen_render::svg_bytes_to_png(png_result.as_bytes());

                // Flush preceding paragraph
                if text_state.has_runs {
                    let prev_p = std::mem::replace(&mut current_paragraph, Paragraph::new());
                    doc = doc.add_paragraph(prev_p);
                    text_state.has_runs = false;
                }

                if let Some((png_bytes, width, height)) = png_result {
                    let (width, height) = fit_image_to_bounds(
                        width,
                        height,
                        max_figure_width_px,
                        max_figure_height_px,
                    );
                    let pic = Pic::new_with_dimensions(png_bytes, width, height);
                    let run = Run::new().add_image(pic);
                    doc = doc
                        .add_paragraph(Paragraph::new().align(AlignmentType::Center).add_run(run));
                    // Inject metadata invisibly so `marksmen-docx-read` can restore the AST.
                    let mut meta_run = Run::new()
                        .vanish()
                        .add_text("```mermaid")
                        .add_break(BreakType::TextWrapping);
                    for line in current_mermaid_source.lines() {
                        meta_run = meta_run.add_text(line).add_break(BreakType::TextWrapping);
                    }
                    meta_run = meta_run.add_text("```");
                    doc = doc.add_paragraph(Paragraph::new().add_run(meta_run));
                } else {
                    // Fallback: raw mermaid source as code text
                    let run = Run::new()
                        .fonts(RunFonts::new().ascii("Consolas"))
                        .add_text(format!("```mermaid\n{}\n```", &current_mermaid_source));
                    doc = doc.add_paragraph(Paragraph::new().add_run(run));
                }
                continue;
            }
            Event::End(TagEnd::CodeBlock) if in_generic_code_block => {
                in_generic_code_block = false;

                // Flush any pending inline paragraph first.
                if text_state.has_runs {
                    let prev_p = std::mem::replace(&mut current_paragraph, Paragraph::new());
                    doc = doc.add_paragraph(prev_p);
                    text_state.has_runs = false;
                }

                // Emit each line as a separate run separated by line-breaks inside
                // a single CodeBlock-styled paragraph so the reader can detect the
                // w:pStyle="CodeBlock" sentinel and reconstruct the fenced block.
                let mut p = Paragraph::new().style("CodeBlock");
                let src = std::mem::take(&mut current_generic_code_source);
                let lines: Vec<&str> = src.split('\n').collect();
                for (i, line) in lines.iter().enumerate() {
                    let mut run = Run::new()
                        .fonts(RunFonts::new().ascii("Consolas").hi_ansi("Consolas"))
                        .add_text(*line);
                    if i + 1 < lines.len() {
                        run = run.add_break(BreakType::TextWrapping);
                    }
                    p = p.add_run(run);
                }
                doc = doc.add_paragraph(p);
                continue;
            }
            Event::InlineMath(latex) => {
                if let Some((png, w, h)) = render_math_to_png(&latex, false) {
                    let (w, h) =
                        fit_image_to_bounds(w, h, max_figure_width_px, max_figure_height_px / 4);
                    current_paragraph = current_paragraph
                        .add_run(Run::new().add_image(Pic::new_with_dimensions(png, w, h)));
                } else {
                    // Fallback: italic Cambria Math text
                    current_paragraph = current_paragraph.add_run(
                        Run::new()
                            .italic()
                            .fonts(
                                RunFonts::new()
                                    .ascii("Cambria Math")
                                    .hi_ansi("Cambria Math"),
                            )
                            .add_text(format!(" {} ", &latex)),
                    );
                }
                continue;
            }
            Event::DisplayMath(latex) => {
                // Flush current paragraph
                if text_state.has_runs {
                    let prev_p = std::mem::replace(&mut current_paragraph, Paragraph::new());
                    doc = doc.add_paragraph(prev_p);
                    text_state.has_runs = false;
                }
                if let Some((png, w, h)) = render_math_to_png(&latex, true) {
                    let (w, h) =
                        fit_image_to_bounds(w, h, max_figure_width_px, max_figure_height_px / 2);
                    doc = doc.add_paragraph(
                        Paragraph::new()
                            .align(AlignmentType::Center)
                            .add_run(Run::new().add_image(Pic::new_with_dimensions(png, w, h))),
                    );
                } else {
                    // Fallback: centred italic paragraph
                    doc = doc.add_paragraph(
                        Paragraph::new().align(AlignmentType::Center).add_run(
                            Run::new()
                                .italic()
                                .fonts(
                                    RunFonts::new()
                                        .ascii("Cambria Math")
                                        .hi_ansi("Cambria Math"),
                                )
                                .add_text(latex.to_string()),
                        ),
                    );
                }
                continue;
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
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
                let run = resolve_image_to_run(
                    img_path_str,
                    &caption,
                    input_dir,
                    max_figure_width_px,
                    max_figure_height_px,
                );
                doc = doc.add_paragraph(Paragraph::new().align(AlignmentType::Center).add_run(run));
                continue;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                in_blockquote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                in_blockquote = false;
            }
            // --- Lists: intercept before handle_event to use DOCX numbering ---
            Event::Start(Tag::List(start_num)) => {
                let is_ordered = start_num.is_some();
                list_ordered_stack.push(is_ordered);
            }
            Event::End(TagEnd::List(_)) => {
                list_ordered_stack.pop();
            }
            Event::Start(Tag::Item) => {
                // Flush any pending paragraph before starting a list item paragraph.
                if text_state.has_runs {
                    doc = doc
                        .add_paragraph(std::mem::replace(&mut current_paragraph, Paragraph::new()));
                    text_state.has_runs = false;
                }
                let depth = list_ordered_stack.len().saturating_sub(1);
                let is_ordered = list_ordered_stack.last().copied().unwrap_or(false);
                let numbering_id = if is_ordered { 2usize } else { 1usize };
                current_paragraph = Paragraph::new()
                    .numbering(NumberingId::new(numbering_id), IndentLevel::new(depth));
            }
            Event::End(TagEnd::Item) => {
                let p = std::mem::replace(&mut current_paragraph, Paragraph::new());
                doc = doc.add_paragraph(p);
                text_state.has_runs = false;
            }
            // --- Native DOCX Footnote Reference (inline superscript anchor) ---
            Event::FootnoteReference(label) => {
                if let Some(&fn_id) = footnote_map.get(label.as_ref()) {
                    // Embed definition content directly so collect_footnotes() can
                    // extract it via FootnoteReference -> Footnote conversion.
                    let content = footnote_content.get(&fn_id).cloned().unwrap_or_default();
                    let footnote = Footnote { id: fn_id, content };
                    let fn_run = Run::new().add_footnote_reference(footnote);
                    current_paragraph = current_paragraph.add_run(fn_run);
                    text_state.has_runs = true;
                } else {
                    // Fallback: superscript text when label has no definition.
                    let mut run = Run::new().add_text(format!("[^{}]", label));
                    run.run_property = run.run_property.vert_align(VertAlignType::SuperScript);
                    current_paragraph = current_paragraph.add_run(run);
                    text_state.has_runs = true;
                }
            }
            // FootnoteDefinition bodies were consumed in the pre-pass; skip here.
            Event::Start(Tag::FootnoteDefinition(_)) => {
                // Drain inner events until the matching end tag.
                loop {
                    match event_iter.next() {
                        None | Some(Event::End(TagEnd::FootnoteDefinition)) => break,
                        _ => {}
                    }
                }
            }
            Event::End(TagEnd::FootnoteDefinition) => {}
            _ => {
                handle_event(
                    event,
                    Container::Doc(&mut doc),
                    &mut current_paragraph,
                    &mut text_state,
                    in_blockquote,
                    config,
                    None,
                );
            }
        }
    }

    // Flush final paragraph if pending (and non-empty)
    if text_state.has_runs {
        doc = doc.add_paragraph(current_paragraph);
    }

    // --- Generate Bibliography ---
    if !text_state.cited_ids.is_empty() {
        let db = load_references(input_dir);
        if !db.is_empty() {
            doc = doc.add_paragraph(
                Paragraph::new()
                    .style("Heading1")
                    .add_run(Run::new().add_text("Bibliography")),
            );

            for (i, id) in text_state.cited_ids.iter().enumerate() {
                if let Some(reference) = db.get(id) {
                    let formatted = format_reference(reference);
                    let p = Paragraph::new().add_run(Run::new().add_text(format!(
                        "[{}] {}",
                        i + 1,
                        formatted
                    )));
                    doc = doc.add_paragraph(p);
                }
            }
        }
    }

    // Write to memory buffer
    let mut docx_buffer = Cursor::new(Vec::new());
    doc.build().pack(&mut docx_buffer)?;

    // ─── Source-DOCX structural passthrough ──────────────────────────────────
    // When the caller supplies the original DOCX bytes, critical XML artifacts
    // that Markdown cannot represent (styles, numbering, settings, theme,
    // header/footer, comment metadata) are reinstated verbatim from the source.
    // This is the canonical approach to lossless roundtrip: the intermediate
    // format carries semantic content; the source ZIP carries structural assets.
    let mut source_archive =
        source_docx.and_then(|b| zip::ZipArchive::new(std::io::Cursor::new(b)).ok());
    // By omitting the few core files authored by marksmen, we effectively
    // blanket-passthrough all advanced Office payload components (e.g.
    // multiple headers, footers, comments, glossaries, endnotes, themes,
    // extensions, docProps, etc.) to guarantee identical structural fidelity.

    // Collect verbatim passthrough bytes from source for each candidate.
    let mut passthrough_map: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();
    // Track which additional files (customXml/*, docMetadata/*, etc.) to inject.
    let extra_files: Vec<(String, Vec<u8>, zip::CompressionMethod)> = Vec::new();

    // Content-type Override entries harvested from source for merge.
    let mut source_ct_overrides: Vec<String> = Vec::new();
    // Relationship entries harvested from source _rels for merge.
    let mut source_rels_entries: Vec<String> = Vec::new();
    let mut source_sect_pr: Option<String> = None;

    // Comment metadata from source for ID reconstruction:
    // normalized-content → (id, author, date, initials)
    let mut src_comment_meta: std::collections::HashMap<String, (String, String, String, String)> =
        std::collections::HashMap::new();

    if let Some(ref mut sa) = source_archive {
        let file_count = sa.len();
        for idx in 0..file_count {
            let (name, _cm, raw) = {
                let mut f = match sa.by_index(idx) {
                    Ok(f) => f,
                    Err(_) => continue,
                };
                let nm = f.name().to_string();
                let compression = f.compression();
                let mut buf = Vec::new();
                let _ = std::io::Read::read_to_end(&mut f, &mut buf);
                (nm, compression, buf)
            };

            // ── Comprehensive Passthrough ──────────────────────────────────────────────
            let is_core_file = name == "word/document.xml"
                || name == "[Content_Types].xml"
                || name == "_rels/.rels"
                || name == "word/_rels/document.xml.rels"
                || name == "word/comments.xml"
                || name == "word/commentsExtended.xml"
                || name.starts_with("word/media/") // allow docx-rs to re-pack media
                || name.ends_with('/'); // implicitly handled directories

            if !is_core_file {
                passthrough_map.insert(name.clone(), raw.clone());
            }

            // ── Track source section formatting (margins, header/footer refs) ─
            if name == "word/document.xml" {
                if let Ok(doc_xml) = String::from_utf8(raw.clone()) {
                    let mut reader = quick_xml::Reader::from_str(&doc_xml);
                    reader.config_mut().trim_text(true);
                    let mut buf = Vec::new();
                    let mut in_sect = false;
                    let mut sect_events = Vec::new();
                    loop {
                        match reader.read_event_into(&mut buf) {
                            Ok(quick_xml::events::Event::Start(ref e))
                                if e.name().as_ref() == b"w:sectPr" =>
                            {
                                in_sect = true;
                                sect_events
                                    .push(quick_xml::events::Event::Start(e.clone().into_owned()));
                            }
                            Ok(quick_xml::events::Event::End(ref e))
                                if e.name().as_ref() == b"w:sectPr" && in_sect =>
                            {
                                sect_events
                                    .push(quick_xml::events::Event::End(e.clone().into_owned()));
                                break;
                            }
                            Ok(quick_xml::events::Event::Empty(ref e))
                                if e.name().as_ref() == b"w:sectPr" =>
                            {
                                sect_events
                                    .push(quick_xml::events::Event::Empty(e.clone().into_owned()));
                                break;
                            }
                            Ok(e) if in_sect => sect_events.push(e.into_owned()),
                            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                            _ => (),
                        }
                        buf.clear();
                    }
                    if !sect_events.is_empty() {
                        let mut out = Vec::new();
                        let mut writer = quick_xml::Writer::new(&mut out);
                        for e in sect_events {
                            let _ = writer.write_event(e);
                        }
                        if let Ok(s) = String::from_utf8(out) {
                            source_sect_pr = Some(s);
                        }
                    }
                }
            }

            // ── Content-type Override harvest ─────────────────────────────────
            if name == "[Content_Types].xml" {
                if let Ok(ct_xml) = String::from_utf8(raw.clone()) {
                    let mut reader = quick_xml::Reader::from_str(&ct_xml);
                    reader.config_mut().trim_text(true);
                    let mut buf = Vec::new();
                    loop {
                        match reader.read_event_into(&mut buf) {
                            Ok(quick_xml::events::Event::Empty(ref e))
                            | Ok(quick_xml::events::Event::Start(ref e)) => {
                                if e.name().as_ref() == b"Override" {
                                    let mut out = Vec::new();
                                    let mut writer = quick_xml::Writer::new(&mut out);
                                    let _ = writer
                                        .write_event(quick_xml::events::Event::Empty(e.clone()));
                                    if let Ok(s) = String::from_utf8(out) {
                                        source_ct_overrides.push(s);
                                    }
                                }
                            }
                            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                            _ => (),
                        }
                        buf.clear();
                    }
                }
            }

            // ── Relationship harvest for header/footer/theme/commentsIds ──────
            if name == "word/_rels/document.xml.rels" {
                if let Ok(rels_xml) = String::from_utf8(raw.clone()) {
                    let mut reader = quick_xml::Reader::from_str(&rels_xml);
                    reader.config_mut().trim_text(true);
                    let mut buf = Vec::new();
                    loop {
                        match reader.read_event_into(&mut buf) {
                            Ok(quick_xml::events::Event::Empty(ref e))
                                if e.name().as_ref() == b"Relationship" =>
                            {
                                let mut out = Vec::new();
                                let mut writer = quick_xml::Writer::new(&mut out);
                                let _ =
                                    writer.write_event(quick_xml::events::Event::Empty(e.clone()));
                                if let Ok(entry) = String::from_utf8(out) {
                                    if !entry.contains("Target=\"styles.xml\"")
                                        && !entry.contains("Target=\"numbering.xml\"")
                                        && !entry.contains("Target=\"fontTable.xml\"")
                                        && !entry.contains("Target=\"settings.xml\"")
                                        && !entry.contains("Target=\"webSettings.xml\"")
                                        && !entry.contains("Target=\"media/")
                                        && !entry.contains("Target=\"comments.xml\"")
                                        && !entry.contains("Target=\"commentsExtended.xml\"")
                                    {
                                        source_rels_entries.push(entry);
                                    }
                                }
                            }
                            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                            _ => (),
                        }
                        buf.clear();
                    }
                }
            }

            // ── Comment ID lookup harvest from source comments.xml ────────────
            if name == "word/comments.xml" {
                if let Ok(cxml) = String::from_utf8(raw) {
                    let mut reader = quick_xml::Reader::from_str(&cxml);
                    reader.config_mut().trim_text(true);
                    let mut buf = Vec::new();

                    let mut current_id = None;
                    let mut in_comment = false;
                    let mut in_t = false;
                    let mut current_text = String::new();

                    loop {
                        match reader.read_event_into(&mut buf) {
                            Ok(quick_xml::events::Event::Start(ref e))
                                if e.name().as_ref() == b"w:comment" =>
                            {
                                in_comment = true;
                                current_text.clear();
                                current_id = e
                                    .attributes()
                                    .filter_map(|a| a.ok())
                                    .find(|a| a.key.as_ref() == b"w:id")
                                    .and_then(|a| String::from_utf8(a.value.into_owned()).ok());
                            }
                            Ok(quick_xml::events::Event::End(ref e))
                                if e.name().as_ref() == b"w:comment" =>
                            {
                                if let Some(id_str) = current_id.take() {
                                    let text_norm = current_text.trim().to_ascii_lowercase();
                                    if let Ok(id_num) = id_str.parse::<usize>() {
                                        if !text_norm.is_empty() {
                                            src_comment_meta.insert(
                                                text_norm,
                                                (
                                                    id_str,
                                                    String::new(),
                                                    String::new(),
                                                    String::new(),
                                                ),
                                            );
                                            let _ = id_num;
                                        }
                                    }
                                }
                                in_comment = false;
                            }
                            Ok(quick_xml::events::Event::Start(ref e))
                                if e.name().as_ref() == b"w:t" && in_comment =>
                            {
                                in_t = true;
                            }
                            Ok(quick_xml::events::Event::End(ref e))
                                if e.name().as_ref() == b"w:t" =>
                            {
                                in_t = false;
                            }
                            Ok(quick_xml::events::Event::Text(ref e)) if in_t => {
                                if let Ok(t) = e.unescape() {
                                    current_text.push_str(&t);
                                }
                            }
                            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                            _ => (),
                        }
                        buf.clear();
                    }
                }
            }
        }
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(docx_buffer.into_inner()))
        .map_err(|e| anyhow::anyhow!("Failed to read generated docx: {}", e))?;

    let mut out_buffer = std::io::Cursor::new(Vec::new());
    let mut zip_writer = zip::ZipWriter::new(&mut out_buffer);

    // Track which generated-file paths have been written so we can inject extras.
    let mut written_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        // Force DEFLATED compression instead of inheriting the docx-rs default,
        // which emits Stored (uncompressed) files leading to bloated outputs.
        let options = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);

        let mut content = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut content)?;
        let path = file.name().to_string();
        written_paths.insert(path.clone());

        // ── Verbatim passthrough: replace generated content with source ──────
        if let Some(src_bytes) = passthrough_map.get(&path) {
            zip_writer.start_file(&path, options)?;
            std::io::Write::write_all(&mut zip_writer, src_bytes)?;
            continue;
        }

        if path == "word/_rels/document.xml.rels" {
            if let Ok(rels_str) = String::from_utf8(content.clone()) {
                let mut reader = quick_xml::Reader::from_str(&rels_str);
                reader.config_mut().trim_text(false);
                let mut buf = Vec::new();
                let mut out = Vec::new();
                let mut writer = quick_xml::Writer::new(&mut out);

                loop {
                    match reader.read_event_into(&mut buf) {
                        Ok(quick_xml::events::Event::Empty(mut e)) => {
                            let mut new_attrs = Vec::new();
                            for attr in e.attributes() {
                                if let Ok(a) = attr {
                                    let k = a.key.as_ref().to_vec();
                                    let mut v = a.value.into_owned();
                                    if k == b"Id" {
                                        let val_str = String::from_utf8_lossy(&v);
                                        if let Some(num_str) = val_str.strip_prefix("rId") {
                                            if let Ok(num) = num_str.parse::<u32>() {
                                                let new_val = format!("rId{}", num + 1000);
                                                v = new_val.into_bytes();
                                            }
                                        }
                                    }
                                    new_attrs.push((k, v));
                                }
                            }
                            e.clear_attributes();
                            for (k, v) in new_attrs {
                                e.push_attribute((k.as_slice(), v.as_slice()));
                            }
                            let _ = writer.write_event(quick_xml::events::Event::Empty(e));
                        }
                        Ok(quick_xml::events::Event::Start(e)) => {
                            let _ = writer.write_event(quick_xml::events::Event::Start(e));
                        }
                        Ok(quick_xml::events::Event::End(e)) => {
                            if e.name().as_ref() == b"Relationships" {
                                for entry in &source_rels_entries {
                                    let mut temp_reader = quick_xml::Reader::from_str(entry);
                                    let mut temp_buf = Vec::new();
                                    if let Ok(quick_xml::events::Event::Empty(te)) =
                                        temp_reader.read_event_into(&mut temp_buf)
                                    {
                                        let _ =
                                            writer.write_event(quick_xml::events::Event::Empty(te));
                                    }
                                }
                            }
                            let _ = writer.write_event(quick_xml::events::Event::End(e));
                        }
                        Ok(quick_xml::events::Event::Eof) => break,
                        Ok(e) => {
                            let _ = writer.write_event(e);
                        }
                        Err(_) => break,
                    }
                    buf.clear();
                }

                zip_writer.start_file(&path, options)?;
                std::io::Write::write_all(&mut zip_writer, &out)?;
            } else {
                zip_writer.start_file(&path, options)?;
                std::io::Write::write_all(&mut zip_writer, &content)?;
            }
            continue;
        }

        // ── [Content_Types].xml: merge source Override entries ────────────────
        if path == "[Content_Types].xml" && !source_ct_overrides.is_empty() {
            if let Ok(ct_str) = String::from_utf8(content.clone()) {
                let mut reader = quick_xml::Reader::from_str(&ct_str);
                reader.config_mut().trim_text(false);
                let mut buf = Vec::new();
                let mut out = Vec::new();
                let mut writer = quick_xml::Writer::new(&mut out);

                let mut existing_parts = std::collections::HashSet::new();

                loop {
                    match reader.read_event_into(&mut buf) {
                        Ok(quick_xml::events::Event::Empty(e)) => {
                            if e.name().as_ref() == b"Override" {
                                if let Some(attr) = e
                                    .attributes()
                                    .filter_map(|a| a.ok())
                                    .find(|a| a.key.as_ref() == b"PartName")
                                {
                                    existing_parts
                                        .insert(String::from_utf8_lossy(&attr.value).into_owned());
                                }
                            }
                            let _ = writer.write_event(quick_xml::events::Event::Empty(e));
                        }
                        Ok(quick_xml::events::Event::End(e)) => {
                            if e.name().as_ref() == b"Types" {
                                for entry in &source_ct_overrides {
                                    let mut temp_reader = quick_xml::Reader::from_str(entry);
                                    let mut temp_buf = Vec::new();
                                    if let Ok(quick_xml::events::Event::Empty(te)) =
                                        temp_reader.read_event_into(&mut temp_buf)
                                    {
                                        if let Some(attr) = te
                                            .attributes()
                                            .filter_map(|a| a.ok())
                                            .find(|a| a.key.as_ref() == b"PartName")
                                        {
                                            let pn =
                                                String::from_utf8_lossy(&attr.value).into_owned();
                                            if !existing_parts.contains(&pn) {
                                                let _ = writer.write_event(
                                                    quick_xml::events::Event::Empty(te),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            let _ = writer.write_event(quick_xml::events::Event::End(e));
                        }
                        Ok(quick_xml::events::Event::Eof) => break,
                        Ok(e) => {
                            let _ = writer.write_event(e);
                        }
                        Err(_) => break,
                    }
                    buf.clear();
                }

                zip_writer.start_file(&path, options)?;
                std::io::Write::write_all(&mut zip_writer, &out)?;
            } else {
                zip_writer.start_file(&path, options)?;
                std::io::Write::write_all(&mut zip_writer, &content)?;
            }
            continue;
        }

        // ── word/document.xml: fix docx-rs <w:del> tag rendering ──────────────
        if path == "word/document.xml" {
            if let Ok(doc_str) = String::from_utf8(content.clone()) {
                let mut reader = quick_xml::Reader::from_str(&doc_str);
                reader.config_mut().trim_text(false);
                let mut buf = Vec::new();
                let mut out = Vec::new();
                let mut writer = quick_xml::Writer::new(&mut out);

                let mut in_del = false;
                let mut skip_sect = false;

                loop {
                    match reader.read_event_into(&mut buf) {
                        Ok(quick_xml::events::Event::Start(mut e)) => {
                            if e.name().as_ref() == b"w:del" {
                                in_del = true;
                            } else if in_del && e.name().as_ref() == b"w:t" {
                                let mut new_e = quick_xml::events::BytesStart::new("w:delText");
                                new_e.push_attribute(("xml:space", "preserve"));
                                e = new_e;
                            } else if e.name().as_ref() == b"w:sectPr" && source_sect_pr.is_some() {
                                skip_sect = true;
                            }

                            if !skip_sect {
                                let mut new_attrs = Vec::new();
                                for attr in e.attributes() {
                                    if let Ok(a) = attr {
                                        let k = a.key.as_ref().to_vec();
                                        let mut v = a.value.into_owned();
                                        if k == b"r:id" || k == b"r:embed" || k == b"r:link" {
                                            let val_str = String::from_utf8_lossy(&v);
                                            if let Some(num_str) = val_str.strip_prefix("rId") {
                                                if let Ok(num) = num_str.parse::<u32>() {
                                                    let new_val = format!("rId{}", num + 1000);
                                                    v = new_val.into_bytes();
                                                }
                                            }
                                        }
                                        new_attrs.push((k, v));
                                    }
                                }
                                e.clear_attributes();
                                for (k, v) in new_attrs {
                                    e.push_attribute((k.as_slice(), v.as_slice()));
                                }
                                let _ = writer.write_event(quick_xml::events::Event::Start(e));
                            }
                        }
                        Ok(quick_xml::events::Event::End(mut e)) => {
                            if e.name().as_ref() == b"w:del" {
                                in_del = false;
                            } else if in_del && e.name().as_ref() == b"w:t" {
                                e = quick_xml::events::BytesEnd::new("w:delText");
                            } else if skip_sect && e.name().as_ref() == b"w:sectPr" {
                                skip_sect = false;
                                if let Some(src_sect) = &source_sect_pr {
                                    let mut temp_reader = quick_xml::Reader::from_str(src_sect);
                                    let mut temp_buf = Vec::new();
                                    loop {
                                        match temp_reader.read_event_into(&mut temp_buf) {
                                            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                                            Ok(ev) => {
                                                let _ = writer.write_event(ev);
                                            }
                                        }
                                        temp_buf.clear();
                                    }
                                }
                                continue;
                            }

                            if !skip_sect {
                                let _ = writer.write_event(quick_xml::events::Event::End(e));
                            }
                        }
                        Ok(quick_xml::events::Event::Empty(mut e)) => {
                            if skip_sect && e.name().as_ref() == b"w:sectPr" {
                                skip_sect = false;
                                if let Some(src_sect) = &source_sect_pr {
                                    let mut temp_reader = quick_xml::Reader::from_str(src_sect);
                                    let mut temp_buf = Vec::new();
                                    loop {
                                        match temp_reader.read_event_into(&mut temp_buf) {
                                            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                                            Ok(ev) => {
                                                let _ = writer.write_event(ev);
                                            }
                                        }
                                        temp_buf.clear();
                                    }
                                }
                                continue;
                            }

                            if !skip_sect {
                                let mut new_attrs = Vec::new();
                                for attr in e.attributes() {
                                    if let Ok(a) = attr {
                                        let k = a.key.as_ref().to_vec();
                                        let mut v = a.value.into_owned();
                                        if k == b"r:id" || k == b"r:embed" || k == b"r:link" {
                                            let val_str = String::from_utf8_lossy(&v);
                                            if let Some(num_str) = val_str.strip_prefix("rId") {
                                                if let Ok(num) = num_str.parse::<u32>() {
                                                    let new_val = format!("rId{}", num + 1000);
                                                    v = new_val.into_bytes();
                                                }
                                            }
                                        }
                                        new_attrs.push((k, v));
                                    }
                                }
                                e.clear_attributes();
                                for (k, v) in new_attrs {
                                    e.push_attribute((k.as_slice(), v.as_slice()));
                                }
                                let _ = writer.write_event(quick_xml::events::Event::Empty(e));
                            }
                        }
                        Ok(quick_xml::events::Event::Eof) => break,
                        Ok(e) => {
                            if !skip_sect {
                                let _ = writer.write_event(e);
                            }
                        }
                        Err(_) => break,
                    }
                    buf.clear();
                }

                zip_writer.start_file(&path, options)?;
                std::io::Write::write_all(&mut zip_writer, &out)?;
            } else {
                zip_writer.start_file(&path, options)?;
                std::io::Write::write_all(&mut zip_writer, &content)?;
            }
            continue;
        }

        // ── Default: emit generated content unchanged ─────────────────────────
        zip_writer.start_file(&path, options)?;
        std::io::Write::write_all(&mut zip_writer, &content)?;
    }

    // ── Inject passthrough files not present in the generated ZIP ─────────────
    // Since we aggressively collect everything except core files, this covers
    // docProps, customXml, headers, footers, themes, and extensions.
    let deflate_opts = zip::write::FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for (fname, fbytes) in &passthrough_map {
        if !written_paths.contains(fname) {
            // Ensure parent directory entry exists.
            if fname.contains('/') {
                let dir_full = &fname[..fname.rfind('/').unwrap() + 1];
                if !written_paths.contains(dir_full) {
                    let _ = zip_writer.add_directory(dir_full, deflate_opts);
                    written_paths.insert(dir_full.to_string());
                }
            }
            zip_writer.start_file(fname, deflate_opts)?;
            std::io::Write::write_all(&mut zip_writer, fbytes)?;
            written_paths.insert(fname.clone());
        }
    }

    for (fname, fbytes, cm) in extra_files {
        if !written_paths.contains(&fname) {
            let opts = zip::write::FileOptions::<()>::default().compression_method(cm);
            if fname.ends_with('/') {
                let _ = zip_writer.add_directory(&fname, opts);
            } else {
                // Ensure parent directory.
                if let Some(slash) = fname.rfind('/') {
                    let dir = &fname[..slash + 1];
                    if !written_paths.contains(dir) {
                        let _ = zip_writer.add_directory(dir, deflate_opts);
                        written_paths.insert(dir.to_string());
                    }
                }
                zip_writer.start_file(&fname, opts)?;
                std::io::Write::write_all(&mut zip_writer, &fbytes)?;
            }
            written_paths.insert(fname);
        }
    }

    zip_writer.finish()?;
    let generated_bytes = out_buffer.into_inner();

    // ─── Template swapping (M26) ──────────────────────────────────────────────
    // When config.template_path is present, inject our generated word/document.xml
    // into a copy of the template archive, preserving all template styling assets.
    if let Some(ref tpl_path) = config.template_path {
        let tpl_bytes = std::fs::read(tpl_path)
            .map_err(|e| anyhow::anyhow!("Failed to read DOCX template '{}': {}", tpl_path, e))?;

        // Extract our word/document.xml from the generated archive.
        let generated_doc_xml: Vec<u8> = {
            let mut gen_archive =
                zip::ZipArchive::new(Cursor::new(&generated_bytes)).map_err(|e| {
                    anyhow::anyhow!("Failed to parse generated DOCX for template swap: {}", e)
                })?;
            let mut doc_xml_file = gen_archive
                .by_name("word/document.xml")
                .map_err(|_| anyhow::anyhow!("word/document.xml missing from generated DOCX"))?;
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut doc_xml_file, &mut buf)?;
            buf
        };

        // Also extract word/comments.xml if present in generated output.
        let generated_comments_xml: Option<Vec<u8>> = {
            let mut gen_archive = zip::ZipArchive::new(Cursor::new(&generated_bytes)).ok();
            gen_archive.as_mut().and_then(|a| {
                if let Ok(mut f) = a.by_name("word/comments.xml") {
                    let mut buf = Vec::new();
                    let _ = std::io::Read::read_to_end(&mut f, &mut buf);
                    if buf.is_empty() {
                        None
                    } else {
                        Some(buf)
                    }
                } else {
                    None
                }
            })
        };

        // Repack: copy every template file, but replace word/document.xml with ours.
        let mut tpl_archive = zip::ZipArchive::new(Cursor::new(&tpl_bytes))
            .map_err(|e| anyhow::anyhow!("Failed to parse DOCX template as ZIP: {}", e))?;
        let mut tpl_out = std::io::Cursor::new(Vec::new());
        let mut tpl_zip = zip::ZipWriter::new(&mut tpl_out);
        let deflate_opts_tpl = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut tpl_written: std::collections::HashSet<String> = std::collections::HashSet::new();

        for i in 0..tpl_archive.len() {
            let (fname, _cm, fbytes) = {
                let mut f = match tpl_archive.by_index(i) {
                    Ok(f) => f,
                    Err(_) => continue,
                };
                let nm = f.name().to_string();
                let cm = f.compression();
                let mut buf = Vec::new();
                let _ = std::io::Read::read_to_end(&mut f, &mut buf);
                (nm, cm, buf)
            };
            if tpl_written.contains(&fname) {
                continue;
            }
            let opts = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Deflated);
            if fname.ends_with('/') {
                let _ = tpl_zip.add_directory(&fname, opts);
            } else {
                tpl_zip.start_file(&fname, opts)?;
                // Swap in our generated document.xml; forward all other template files.
                if fname == "word/document.xml" {
                    std::io::Write::write_all(&mut tpl_zip, &generated_doc_xml)?;
                } else if fname == "word/comments.xml" {
                    if let Some(ref cxml) = generated_comments_xml {
                        std::io::Write::write_all(&mut tpl_zip, cxml)?;
                    } else {
                        std::io::Write::write_all(&mut tpl_zip, &fbytes)?;
                    }
                } else {
                    std::io::Write::write_all(&mut tpl_zip, &fbytes)?;
                }
            }
            tpl_written.insert(fname);
        }

        // Ensure word/document.xml exists even if the template lacked it.
        if !tpl_written.contains("word/document.xml") {
            tpl_zip.start_file("word/document.xml", deflate_opts_tpl)?;
            std::io::Write::write_all(&mut tpl_zip, &generated_doc_xml)?;
        }

        tpl_zip.finish()?;
        let raw_zip = tpl_out.into_inner();

        // Stage 3: Agile Encryption (OLE2 container)
        if let Some(pwd) = &config.password {
            let mut encrypted_buffer = std::io::Cursor::new(Vec::new());
            if marksmen_crypto::protect_docx(
                std::io::Cursor::new(&raw_zip),
                &mut encrypted_buffer,
                pwd,
            )
            .is_ok()
            {
                return Ok(encrypted_buffer.into_inner());
            }
        }

        return Ok(raw_zip);
    }

    // Stage 3: Agile Encryption for non-template path
    if let Some(pwd) = &config.password {
        let mut encrypted_buffer = std::io::Cursor::new(Vec::new());
        if marksmen_crypto::protect_docx(
            std::io::Cursor::new(&generated_bytes),
            &mut encrypted_buffer,
            pwd,
        )
        .is_ok()
        {
            return Ok(encrypted_buffer.into_inner());
        }
    }

    Ok(generated_bytes)
}

// svg_to_png and render_graph_to_svg removed: canonical implementations
// live in marksmen_render::svg_bytes_to_png and marksmen_render::mermaid::render_graph_to_svg.

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
    let content_width_twips =
        page_width_twips.saturating_sub(margin_left_twips + margin_right_twips);
    let content_height_twips =
        page_height_twips.saturating_sub(margin_top_twips + margin_bottom_twips);

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

fn resolve_image_to_run(
    img_path_str: &str,
    caption: &str,
    input_dir: &Path,
    max_figure_width_px: u32,
    max_figure_height_px: u32,
) -> Run {
    let is_mmd = img_path_str.ends_with(".mmd");
    if is_mmd {
        // ... handled below but we need to rearrange since we want data uri check first
    }

    if img_path_str.starts_with("data:image/") {
        if let Some(comma_idx) = img_path_str.find(',') {
            let base64_data = &img_path_str[comma_idx + 1..];
            use base64::Engine as _;
            if let Ok(raw_bytes) = base64::engine::general_purpose::STANDARD.decode(base64_data) {
                let is_svg = img_path_str.starts_with("data:image/svg+xml");
                let (png_bytes, width, height) = if is_svg {
                    match marksmen_render::svg_bytes_to_png(&raw_bytes) {
                        Some(result) => result,
                        None => {
                            return Run::new().add_text(format!(
                                "![{}]({})",
                                caption, "data:image/svg+xml;base64,..."
                            ))
                        }
                    }
                } else {
                    let (w, h) = image_dimensions(&raw_bytes).unwrap_or((640, 480));
                    (raw_bytes, w, h)
                };

                let (width, height) =
                    fit_image_to_bounds(width, height, max_figure_width_px, max_figure_height_px);
                return Run::new().add_image(Pic::new_with_dimensions(png_bytes, width, height));
            }
        }
    }

    let resolved = if Path::new(img_path_str).is_absolute() {
        PathBuf::from(img_path_str)
    } else {
        input_dir.join(img_path_str)
    };

    if is_mmd {
        return match std::fs::read_to_string(&resolved)
            .ok()
            .and_then(|src| render_mmd_to_png(&src))
        {
            Some((png, w, h)) => {
                let (w, h) = fit_image_to_bounds(w, h, max_figure_width_px, max_figure_height_px);
                Run::new().add_image(Pic::new_with_dimensions(png, w, h))
            }
            None => Run::new()
                .italic()
                .add_text(format!("[Diagram: {}]", caption)),
        };
    }

    if let Ok(raw_bytes) = std::fs::read(&resolved) {
        let is_svg = img_path_str.ends_with(".svg")
            || raw_bytes.starts_with(b"<?xml")
            || raw_bytes.starts_with(b"<svg");

        let (png_bytes, width, height) = if is_svg {
            match svg_bytes_to_png(&raw_bytes) {
                Some(result) => result,
                None => return Run::new().add_text(format!("![{}]({})", caption, img_path_str)),
            }
        } else {
            let (w, h) = image_dimensions(&raw_bytes).unwrap_or((640, 480));
            (raw_bytes, w, h)
        };

        let (width, height) =
            fit_image_to_bounds(width, height, max_figure_width_px, max_figure_height_px);
        Run::new().add_image(Pic::new_with_dimensions(png_bytes, width, height))
    } else {
        Run::new()
            .italic()
            .add_text(format!("[Missing image: {}]", img_path_str))
    }
}
