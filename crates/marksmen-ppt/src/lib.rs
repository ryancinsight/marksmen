//! PowerPoint Open XML (PPTX) target for the marksmen workspace.
//!
//! # Slide Segmentation Contract
//!
//! The Markdown AST is partitioned into discrete slides at two boundaries:
//! - `Event::Rule` (`---`) — explicit slide separator.
//! - `Event::Start(Tag::Heading { level: H1, .. })` — each H1 title opens a
//!   new slide, placing the heading text in the title placeholder and the
//!   subsequent block content in the body placeholder.
//!
//! All other heading levels (H2–H6) are rendered as bold body paragraphs
//! inside the current slide's body placeholder.

use anyhow::{Context, Result};
use marksmen_core::config::Config;
use pulldown_cmark::{Event, HeadingLevel, Tag, TagEnd};
use std::io::Write;
use zip::{ZipWriter, write::SimpleFileOptions};

pub mod ooxml;

/// Converts a `pulldown-cmark` event stream into a `.pptx` binary payload.
///
/// # Invariant
/// The returned `Vec<u8>` is a valid, self-contained OOXML Presentation
/// archive, readable by Microsoft PowerPoint 2016+ and LibreOffice Impress.
pub fn convert(events: &[Event<'_>], config: &Config) -> Result<Vec<u8>> {
    let slides = segment_slides(events, config);
    pack_pptx(&slides)
}

// ── Slide data model ─────────────────────────────────────────────────────────

/// Discriminated union of renderable slide content items.
enum SlideContent {
    Para(BodyParagraph),
    Table(SlideTable),
    Math(String),
    MermaidPlaceholder(String),
}

#[derive(Default, PartialEq, Eq)]
enum SlideLayout {
    Title,
    #[default]
    Content,
}

/// A single presentation slide.
#[derive(Default)]
struct Slide {
    title: String,
    layout: SlideLayout,
    body: Vec<SlideContent>,
}

/// A table captured from the AST for OOXML rendering as a DrawingML graphic frame.
struct SlideTable {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[derive(Debug)]
struct BodyParagraph {
    runs: Vec<Run>,
    /// Indentation level (for bullet nesting).
    indent: u32,
    /// List prefix appended before the first run, e.g. `"• "` or `"1. "`.
    prefix: Option<String>,
    /// When true the paragraph is styled as a heading (bold, larger).
    is_heading: bool,
    /// Font size override in half-points (0 = use default).
    heading_sz: u32,
}

impl BodyParagraph {
    fn plain(text: impl Into<String>, indent: u32) -> Self {
        Self {
            runs: vec![Run {
                text: text.into(),
                bold: false,
                italic: false,
                code: false,
            }],
            indent,
            prefix: None,
            is_heading: false,
            heading_sz: 0,
        }
    }
}

#[derive(Debug, Default)]
struct Run {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
}

// ── AST → Slides segmentation ────────────────────────────────────────────────

fn segment_slides(events: &[Event<'_>], config: &Config) -> Vec<Slide> {
    let mut slides: Vec<Slide> = Vec::new();
    let mut current: Slide = Slide::default();

    // Set title slide content from config if available.
    if !config.title.is_empty() {
        current.title = config.title.clone();
        if !config.author.is_empty() {
            current.body.push(SlideContent::Para(BodyParagraph::plain(
                config.author.to_string(),
                0,
            )));
        }
        if !config.date.is_empty() {
            current.body.push(SlideContent::Para(BodyParagraph::plain(
                config.date.to_string(),
                0,
            )));
        }
    }

    let mut in_title_h1 = false;
    let mut current_para: BodyParagraph = BodyParagraph {
        runs: Vec::new(),
        indent: 0,
        prefix: None,
        is_heading: false,
        heading_sz: 0,
    };
    let mut bold = false;
    let mut italic = false;
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_block_buf = String::new();

    // Table accumulation state.
    let mut in_table = false;
    let mut table_headers: Vec<String> = Vec::new();
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();
    let mut in_table_head = false;

    for event in events.iter().cloned() {
        // ── Table events (highest priority guard) ────────────────────────
        if in_table {
            match &event {
                Event::Start(Tag::TableHead) => {
                    in_table_head = true;
                }
                Event::End(TagEnd::TableHead) => {
                    in_table_head = false;
                    table_headers = std::mem::take(&mut current_row)
                        .into_iter()
                        .map(|c| c.trim().to_string())
                        .collect();
                }
                Event::Start(Tag::TableRow) => {
                    current_row.clear();
                }
                Event::End(TagEnd::TableRow)
                    if !in_table_head => {
                        table_rows.push(
                            std::mem::take(&mut current_row)
                                .into_iter()
                                .map(|c| c.trim().to_string())
                                .collect(),
                        );
                    }
                Event::Start(Tag::TableCell) => {
                    current_cell.clear();
                }
                Event::End(TagEnd::TableCell) => {
                    current_row.push(std::mem::take(&mut current_cell));
                }
                Event::Text(t) => {
                    current_cell.push_str(t.as_ref());
                }
                Event::Code(t) => {
                    current_cell.push_str(t.as_ref());
                }
                Event::End(TagEnd::Table) => {
                    in_table = false;
                    current.body.push(SlideContent::Table(SlideTable {
                        headers: std::mem::take(&mut table_headers),
                        rows: std::mem::take(&mut table_rows),
                    }));
                }
                _ => {}
            }
            continue;
        }

        match event {
            // ── Slide boundaries ──────────────────────────────────────────
            Event::Rule => {
                flush_para(&mut current_para, &mut current);
                slides.push(std::mem::take(&mut current));
            }

            // ── Tables ────────────────────────────────────────────────────
            Event::Start(Tag::Table(_)) => {
                flush_para(&mut current_para, &mut current);
                in_table = true;
                table_headers.clear();
                table_rows.clear();
                current_row.clear();
                current_cell.clear();
            }

            // ── Headings ──────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                flush_para(&mut current_para, &mut current);
                if level == HeadingLevel::H1 || level == HeadingLevel::H2 {
                    if !current.title.is_empty() || !current.body.is_empty() {
                        slides.push(std::mem::take(&mut current));
                    }
                    in_title_h1 = true;
                    current.layout = if level == HeadingLevel::H1 {
                        SlideLayout::Title
                    } else {
                        SlideLayout::Content
                    };
                } else {
                    current_para.is_heading = true;
                    current_para.heading_sz = match level {
                        HeadingLevel::H3 => 2800,
                        _ => 2400,
                    };
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if in_title_h1 {
                    in_title_h1 = false;
                } else {
                    flush_para(&mut current_para, &mut current);
                }
            }

            // ── Paragraphs ────────────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush_para(&mut current_para, &mut current);
            }

            // ── Lists ─────────────────────────────────────────────────────
            Event::Start(Tag::List(start)) => {
                list_stack.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
            }
            Event::Start(Tag::Item) => {
                flush_para(&mut current_para, &mut current);
                let depth = list_stack.len().saturating_sub(1) as u32;
                current_para.indent = depth;
                match list_stack.last() {
                    Some(Some(n)) => {
                        current_para.prefix = Some(format!("{}. ", n));
                        if let Some(Some(counter)) = list_stack.last_mut() {
                            *counter += 1;
                        }
                    }
                    _ => {
                        current_para.prefix = Some("• ".to_string());
                    }
                }
            }
            Event::End(TagEnd::Item) => {
                flush_para(&mut current_para, &mut current);
            }

            // ── Code blocks ───────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(ref kind)) => {
                flush_para(&mut current_para, &mut current);
                code_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.as_ref().to_string(),
                    _ => String::new(),
                };
                in_code_block = true;
                code_block_buf.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                if code_lang == "mermaid" {
                    current.body.push(SlideContent::MermaidPlaceholder(format!(
                        "[Mermaid diagram: {}]",
                        code_block_buf.lines().next().unwrap_or("").trim()
                    )));
                } else {
                    for line in code_block_buf.lines() {
                        current.body.push(SlideContent::Para(BodyParagraph {
                            runs: vec![Run {
                                text: line.to_string(),
                                bold: false,
                                italic: false,
                                code: true,
                            }],
                            indent: 0,
                            prefix: None,
                            is_heading: false,
                            heading_sz: 0,
                        }));
                    }
                }
                code_block_buf.clear();
            }

            // ── Inline ────────────────────────────────────────────────────
            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Start(Tag::Strikethrough) | Event::End(TagEnd::Strikethrough) => {}
            Event::Start(Tag::Link { .. }) | Event::End(TagEnd::Link) => {}
            Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => {}

            Event::Code(text) => {
                push_run(&mut current_para, text.as_ref(), bold, italic, true);
            }
            Event::Text(text) => {
                if in_code_block {
                    code_block_buf.push_str(text.as_ref());
                } else if in_title_h1 {
                    current.title.push_str(text.as_ref());
                } else {
                    push_run(&mut current_para, text.as_ref(), bold, italic, false);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_code_block {
                    code_block_buf.push('\n');
                } else {
                    push_run(&mut current_para, " ", false, false, false);
                }
            }
            // Math: display as styled paragraph with formula notation.
            Event::InlineMath(m) => {
                push_run(
                    &mut current_para,
                    &format!("${}$", m.as_ref()),
                    false,
                    true,
                    true,
                );
            }
            Event::DisplayMath(m) => {
                flush_para(&mut current_para, &mut current);
                current
                    .body
                    .push(SlideContent::Math(m.as_ref().to_string()));
            }

            _ => {}
        }
    }

    flush_para(&mut current_para, &mut current);
    if !current.title.is_empty() || !current.body.is_empty() {
        slides.push(current);
    }
    if slides.is_empty() {
        slides.push(Slide::default());
    }
    slides
}

fn push_run(para: &mut BodyParagraph, text: &str, bold: bool, italic: bool, code: bool) {
    para.runs.push(Run {
        text: text.to_string(),
        bold,
        italic,
        code,
    });
}

fn flush_para(para: &mut BodyParagraph, slide: &mut Slide) {
    let has_content = para.runs.iter().any(|r| !r.text.trim().is_empty());
    if has_content || para.prefix.is_some() {
        slide.body.push(SlideContent::Para(std::mem::replace(
            para,
            BodyParagraph {
                runs: Vec::new(),
                indent: 0,
                prefix: None,
                is_heading: false,
                heading_sz: 0,
            },
        )));
    } else {
        *para = BodyParagraph {
            runs: Vec::new(),
            indent: 0,
            prefix: None,
            is_heading: false,
            heading_sz: 0,
        };
    }
}

// ── OOXML packing ─────────────────────────────────────────────────────────────

/// EMU bounds for a standard 10in × 7.5in widescreen slide.
const SLIDE_CX: i64 = 9_144_000; // 10 in * 914400 EMU/in
const SLIDE_CY: i64 = 6_858_000; // 7.5 in * 914400 EMU/in

/// Title placeholder geometry (top band).
// const TITLE_X: i64 = 457_200;
// const TITLE_Y: i64 = 274_638;
// const TITLE_CX: i64 = 8_229_600;
// const TITLE_CY: i64 = 1_143_000;

/// Body placeholder geometry (lower portion).
const BODY_X: i64 = 457_200;
const BODY_Y: i64 = 1_600_200;
const BODY_CX: i64 = 8_229_600;
const BODY_CY: i64 = 4_525_963;

fn pack_pptx(slides: &[Slide]) -> Result<Vec<u8>> {
    let buf = Vec::new();
    let cursor = std::io::Cursor::new(buf);
    let mut zip = ZipWriter::new(cursor);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // ── [Content_Types].xml ──────────────────────────────────────────────────
    let mut content_types = ooxml::CONTENT_TYPES.to_string();
    for (i, _) in slides.iter().enumerate() {
        let n = i + 1;
        content_types.push_str(&format!(
            r#"  <Override PartName="/ppt/slides/slide{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#
        ));
        content_types.push('\n');
    }
    content_types.push_str("</Types>");
    zip.start_file("[Content_Types].xml", opts)?;
    zip.write_all(content_types.as_bytes())?;

    // ── _rels/.rels ───────────────────────────────────────────────────────────
    zip.start_file("_rels/.rels", opts)?;
    zip.write_all(ooxml::ENVELOPE_RELS.as_bytes())?;

    // ── ppt/presentation.xml ─────────────────────────────────────────────────
    let mut sld_id_list = String::new();
    let mut pres_rels = ooxml::PRESENTATION_RELS.to_string();
    for (i, _) in slides.iter().enumerate() {
        let n = i + 1;
        let rid = n + 2; // rId1=slideMaster, rId2=theme, rId3+ = slides
        sld_id_list.push_str(&format!(
            r#"    <p:sldId id="{}" r:id="rId{}"/>"#,
            256 + n,
            rid
        ));
        sld_id_list.push('\n');
        pres_rels.push_str(&format!(
            r#"  <Relationship Id="rId{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{n}.xml"/>"#
        ));
        pres_rels.push('\n');
    }
    pres_rels.push_str("</Relationships>");

    let presentation_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
                xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
                xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                saveSubsetFonts="1">
  <p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
  <p:sldSz cx="{SLIDE_CX}" cy="{SLIDE_CY}" type="screen16x9"/>
  <p:notesSz cx="6858000" cy="9144000"/>
  <p:sldIdLst>
{sld_id_list}  </p:sldIdLst>
</p:presentation>"#
    );
    zip.start_file("ppt/presentation.xml", opts)?;
    zip.write_all(presentation_xml.as_bytes())?;

    // ── ppt/_rels/presentation.xml.rels ──────────────────────────────────────
    zip.start_file("ppt/_rels/presentation.xml.rels", opts)?;
    zip.write_all(pres_rels.as_bytes())?;

    // ── Static assets ────────────────────────────────────────────────────────
    zip.start_file("ppt/slideMasters/slideMaster1.xml", opts)?;
    zip.write_all(ooxml::SLIDE_MASTER.as_bytes())?;

    zip.start_file("ppt/slideMasters/_rels/slideMaster1.xml.rels", opts)?;
    zip.write_all(ooxml::SLIDE_MASTER_RELS.as_bytes())?;

    zip.start_file("ppt/slideLayouts/slideLayout1.xml", opts)?;
    zip.write_all(ooxml::SLIDE_LAYOUT.as_bytes())?;

    zip.start_file("ppt/slideLayouts/_rels/slideLayout1.xml.rels", opts)?;
    zip.write_all(ooxml::SLIDE_LAYOUT_RELS.as_bytes())?;

    zip.start_file("ppt/theme/theme1.xml", opts)?;
    zip.write_all(ooxml::THEME.as_bytes())?;

    // ── Slide XML files ───────────────────────────────────────────────────────
    for (i, slide) in slides.iter().enumerate() {
        let n = i + 1;
        let slide_xml = render_slide_xml(slide);
        zip.start_file(format!("ppt/slides/slide{n}.xml"), opts)?;
        zip.write_all(slide_xml.as_bytes())?;

        // Each slide needs a .rels pointing to its layout.
        let slide_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#.to_string();
        zip.start_file(format!("ppt/slides/_rels/slide{n}.xml.rels"), opts)?;
        zip.write_all(slide_rels.as_bytes())?;
    }

    let finished = zip.finish().context("failed to finalize PPTX zip")?;
    Ok(finished.into_inner())
}

// ── Slide XML rendering ──────────────────────────────────────────────────────

fn render_slide_xml(slide: &Slide) -> String {
    let title_xml = render_title_shape(&slide.title, &slide.layout);

    // Partition body items: paragraphs go into the text body shape;
    // tables and other graphic elements go after as separate spTree children.
    let mut extra_shapes = String::new();
    let mut table_shape_id = 4u32;

    // First pass: emit all paragraphs; collect non-para items for extra shapes.
    let mut paras_xml = String::new();
    for item in &slide.body {
        match item {
            SlideContent::Para(p) => {
                paras_xml.push_str(&render_paragraph(p));
            }
            SlideContent::Math(m) => {
                // Display math: monospace centered paragraph with notation.
                let escaped = xml_escape(&format!("\u{1D6E2} {}", m));
                paras_xml.push_str(&format!(
                    r#"          <a:p>
            <a:pPr algn="ctr"/>
            <a:r><a:rPr lang="en-US" dirty="0"><a:latin typeface="Courier New"/></a:rPr><a:t>{escaped}</a:t></a:r>
          </a:p>
"#
                ));
            }
            SlideContent::MermaidPlaceholder(label) => {
                let escaped = xml_escape(label);
                paras_xml.push_str(&format!(
                    r#"          <a:p>
            <a:pPr/>
            <a:r><a:rPr lang="en-US" i="1" dirty="0"/><a:t>{escaped}</a:t></a:r>
          </a:p>
"#
                ));
            }
            SlideContent::Table(t) => {
                extra_shapes.push_str(&render_table_shape(t, table_shape_id));
                table_shape_id += 1;
            }
        }
    }

    let body_xml = format!(
        r#"      <p:sp>
        <p:nvSpPr>
          <p:cNvPr id="3" name="Body"/>
          <p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr>
          <p:nvPr><p:ph idx="1"/></p:nvPr>
        </p:nvSpPr>
        <p:spPr>
          <a:xfrm><a:off x="{BODY_X}" y="{BODY_Y}"/><a:ext cx="{BODY_CX}" cy="{BODY_CY}"/></a:xfrm>
        </p:spPr>
        <p:txBody>
          <a:bodyPr/>
          <a:lstStyle/>
{paras_xml}
        </p:txBody>
      </p:sp>"#,
    );


    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
       xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr>
        <p:cNvPr id="1" name=""/>
        <p:cNvGrpSpPr/>
        <p:nvPr/>
      </p:nvGrpSpPr>
      <p:grpSpPr>
        <a:xfrm>
          <a:off x="0" y="0"/>
          <a:ext cx="{SLIDE_CX}" cy="{SLIDE_CY}"/>
          <a:chOff x="0" y="0"/>
          <a:chExt cx="{SLIDE_CX}" cy="{SLIDE_CY}"/>
        </a:xfrm>
      </p:grpSpPr>
{title_xml}
{body_xml}
{extra_shapes}
    </p:spTree>
  </p:cSld>
</p:sld>"#
    )
}

fn render_title_shape(title: &str, layout: &SlideLayout) -> String {
    let escaped = xml_escape(title);

    let (tx, ty, tcx, tcy, ppr) = match layout {
        SlideLayout::Title => {
            // Centered huge title
            (
                457_200,
                1_600_200,
                8_229_600,
                2_000_000,
                r#"<a:pPr algn="ctr"/>"#,
            )
        }
        SlideLayout::Content => {
            // Standard top title
            (457_200, 274_320, 8_229_600, 1_143_000, r#"<a:pPr/>"#)
        }
    };

    format!(
        r#"      <p:sp>
        <p:nvSpPr>
          <p:cNvPr id="2" name="Title"/>
          <p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr>
          <p:nvPr><p:ph type="title"/></p:nvPr>
        </p:nvSpPr>
        <p:spPr>
          <a:xfrm><a:off x="{tx}" y="{ty}"/><a:ext cx="{tcx}" cy="{tcy}"/></a:xfrm>
        </p:spPr>
        <p:txBody>
          <a:bodyPr/>
          <a:lstStyle/>
          <a:p>{ppr}<a:r><a:rPr lang="en-US" sz="4400" b="1" dirty="0"/><a:t>{escaped}</a:t></a:r></a:p>
        </p:txBody>
      </p:sp>"#
    )
}

/// Render a DrawingML table as a graphic-frame shape.
///
/// The table is placed below the body area. Column width is distributed
/// evenly across the slide width minus margins.
fn render_table_shape(table: &SlideTable, shape_id: u32) -> String {
    // Geometry: place below standard body — or overlap if slide is short.
    const TBL_X: i64 = 457_200;
    const TBL_Y: i64 = 4_800_000;
    const TBL_CX: i64 = 8_229_600;
    // Row height: 400_000 EMU ≈ 0.44 in
    const ROW_H: i64 = 400_000;

    let col_count = table.headers.len().max(1);
    let col_w = TBL_CX / col_count as i64;
    let row_count = table.rows.len() + 1; // +1 for header
    let tbl_cy = ROW_H * row_count as i64;

    // Grid column definitions.
    let mut grid_xml = String::new();
    for _ in 0..col_count {
        grid_xml.push_str(&format!("        <a:gridCol w=\"{col_w}\"/>\n"));
    }

    // Row renderer.
    let render_row = |cells: &[String], is_header: bool| -> String {
        let mut row = format!("        <a:tr h=\"{ROW_H}\">\n");
        for (ci, cell) in cells.iter().enumerate() {
            let text = xml_escape(cell);
            let bold_attr = if is_header { r#" b="1""# } else { "" };
            let bg = if is_header {
                "<a:tcPr><a:solidFill><a:srgbClr val=\"4472C4\"/></a:solidFill></a:tcPr>"
            } else if ci % 2 == 0 {
                "<a:tcPr/>"
            } else {
                "<a:tcPr/>"
            };
            let _ = ci;
            row.push_str(&format!(
                "          <a:tc>\n\
                   <a:txBody><a:bodyPr/><a:lstStyle/>\n\
                   <a:p><a:r><a:rPr lang=\"en-US\"{bold_attr} dirty=\"0\"/><a:t>{text}</a:t></a:r></a:p>\n\
                   </a:txBody>\n\
                   {bg}\n\
                   </a:tc>\n"
            ));
        }
        // Pad missing cells.
        for _ in cells.len()..col_count {
            row.push_str("          <a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p/></a:txBody><a:tcPr/></a:tc>\n");
        }
        row.push_str("        </a:tr>\n");
        row
    };

    let mut rows_xml = render_row(&table.headers, true);
    for row in &table.rows {
        rows_xml.push_str(&render_row(row, false));
    }

    format!(
        r#"      <p:graphicFrame>
        <p:nvGraphicFramePr>
          <p:cNvPr id="{shape_id}" name="Table {shape_id}"/>
          <p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr>
          <p:nvPr/>
        </p:nvGraphicFramePr>
        <p:xfrm>
          <a:off x="{TBL_X}" y="{TBL_Y}"/>
          <a:ext cx="{TBL_CX}" cy="{tbl_cy}"/>
        </p:xfrm>
        <a:graphic>
          <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table">
            <a:tbl>
              <a:tblPr firstRow="1" bandRow="1">
                <a:tableStyleId>{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}</a:tableStyleId>
              </a:tblPr>
              <a:tblGrid>
{grid_xml}
              </a:tblGrid>
{rows_xml}
            </a:tbl>
          </a:graphicData>
        </a:graphic>
      </p:graphicFrame>"#
    )
}

fn render_paragraph(para: &BodyParagraph) -> String {
    // Build pPr: indent in EMU (1 level = 457200 EMU ≈ 0.5in)
    let margin_l = 342900u32 + para.indent * 457200;
    let indent_val = if para.prefix.is_some() {
        -(342900i32)
    } else {
        0
    };
    let lvl = para.indent;

    let heading_sz_attr = if para.heading_sz > 0 {
        format!(r#" sz="{}""#, para.heading_sz)
    } else {
        String::new()
    };

    let mut runs_xml = String::new();

    // Prefix run (bullet or number).
    if let Some(ref prefix) = para.prefix {
        let escaped = xml_escape(prefix);
        runs_xml.push_str(&format!(
            r#"          <a:r><a:rPr lang="en-US" dirty="0"/><a:t>{escaped}</a:t></a:r>"#
        ));
    }

    for run in &para.runs {
        let escaped = xml_escape(&run.text);
        let b = if run.bold { r#" b="1""# } else { "" };
        let i = if run.italic { r#" i="1""# } else { "" };
        let typeface = if run.code {
            r#"<a:latin typeface="Courier New"/>"#
        } else {
            ""
        };
        runs_xml.push_str(&format!(
            r#"          <a:r><a:rPr lang="en-US"{b}{i}{heading_sz_attr} dirty="0">{typeface}</a:rPr><a:t>{escaped}</a:t></a:r>"#
        ));
    }

    format!(
        r#"          <a:p>
            <a:pPr marL="{margin_l}" indent="{indent_val}" lvl="{lvl}"/>
{runs_xml}
          </a:p>
"#
    )
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use marksmen_core::parsing::parser;
    use std::io::Cursor;

    #[test]
    fn test_pptx_zip_valid() {
        let md = "# Slide One\nFirst content paragraph.\n\n---\n\n# Slide Two\nSecond content.\n\n- item a\n- item b";
        let events = parser::parse(md);
        let bytes = convert(&events, &Config::default()).expect("pptx conversion failed");
        // Validate: must unpack as a valid zip archive.
        let cursor = Cursor::new(&bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("output is not a valid zip");
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"[Content_Types].xml".to_string()));
        assert!(names.contains(&"ppt/presentation.xml".to_string()));
        assert!(names.contains(&"ppt/slides/slide1.xml".to_string()));
        assert!(names.contains(&"ppt/slides/slide2.xml".to_string()));
    }

    #[test]
    fn test_slide_count_from_h1() {
        let md = "# Title\nintro\n\n# Second Slide\nbody\n\n# Third\nmore";
        let events = parser::parse(md);
        let slides = segment_slides(&events, &Config::default());
        assert_eq!(slides.len(), 3);
        assert_eq!(slides[0].title.trim(), "Title");
        assert_eq!(slides[1].title.trim(), "Second Slide");
    }

    #[test]
    fn test_slide_count_from_rule() {
        let md = "## Content A\nbody a\n\n---\n\n## Content B\nbody b";
        let events = parser::parse(md);
        let slides = segment_slides(&events, &Config::default());
        assert_eq!(slides.len(), 2);
    }
}
