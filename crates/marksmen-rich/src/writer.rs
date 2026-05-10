//! Markdown → RTF 1.9 generator.
//!
//! # Invariants
//! - Output is valid RTF 1.9 (ANSI encoding, signed Unicode escapes).
//! - Font table: f0=Times New Roman, f1=Courier New, f2=Symbol (bullets), f3=Arial.
//! - Colour table: idx1=black, idx2=dark-grey (code), idx3=navy (links).
//! - Lists use \listtable/\listoverridetable; items reference \ls1 (ordered) or \ls2 (unordered).
//! - Tables are fully buffered; \cellx widths are computed from the actual column count.
//! - Footnote definitions are pre-scanned; emitted as a "Footnotes" section at the end.
//! - Data-URI images are embedded as \pict\pngblip hex blocks.
//! - Math is wrapped in {\*\momath} plus a visible monospace fallback run.

use anyhow::Result;
use marksmen_core::Config;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use std::collections::HashMap;

/// US-Letter body width in twips (8.5 in − 2×1 in margins = 6.5 in × 1440).
const BODY_W: u32 = 9360;

pub fn convert(events: &[Event<'_>], config: &Config) -> Result<Vec<u8>> {
    let footnotes = collect_footnotes(&events);
    let mut buf = String::with_capacity(32768);
    emit_header(&mut buf, config);
    let mut s = WriterState::new(footnotes);
    for ev in events {
        write_event(&mut buf, ev, &mut s);
    }
    if s.in_table {
        flush_table(&mut buf, &mut s);
    }
    if s.in_paragraph {
        buf.push_str("\\par\n");
    }
    if !s.footnote_entries.is_empty() {
        emit_footnote_section(&mut buf, &s);
    }
    buf.push('}');
    Ok(buf.into_bytes())
}

// ── Document header ──────────────────────────────────────────────────────────

fn emit_header(buf: &mut String, config: &Config) {
    buf.push_str("{\\rtf1\\ansi\\ansicpg1252\\deff0\\deflang1033\n");

    // Font table
    buf.push_str("{\\fonttbl");
    buf.push_str("{\\f0\\froman\\fcharset0 Times New Roman;}");
    buf.push_str("{\\f1\\fmodern\\fcharset0 Courier New;}");
    buf.push_str("{\\f2\\fnil\\fcharset2 Symbol;}");
    buf.push_str("{\\f3\\fswiss\\fcharset0 Arial;}");
    buf.push_str("}\n");

    // Colour table (index 1-based: ;=black, ;=dark-grey, ;=navy)
    buf.push_str("{\\colortbl;\\red0\\green0\\blue0;\\red64\\green64\\blue64;\\red0\\green0\\blue153;\\red200\\green0\\blue0;}\n");

    // Stylesheet
    buf.push_str("{\\stylesheet\n");
    buf.push_str("{\\s0\\widctlpar\\adjustright\\f0\\fs24\\lang1033 Normal;}\n");
    buf.push_str("{\\*\\cs10\\additive Default Paragraph Font;}\n");
    for (s, fs, sb, sa) in [
        (1u8, 48u32, 240, 60),
        (2, 40, 200, 60),
        (3, 32, 180, 60),
        (4, 28, 160, 60),
        (5, 24, 140, 60),
        (6, 20, 120, 60),
    ] {
        buf.push_str(&format!(
            "{{\\s{s}\\sb{sb}\\sa{sa}\\keepn\\widctlpar\\adjustright\\b\\fs{fs}\\lang1033\\sbasedon0\\snext0 heading {s};}}\n"
        ));
    }
    buf.push_str("}\n");

    // List table: list 1=ordered decimal, list 2=unordered bullet
    buf.push_str("{\\*\\listtable\n");
    // Ordered: 9 levels of decimal numbering
    buf.push_str("{\\list\\listtemplateid1\\listhybrid\n");
    for lvl in 0u8..9 {
        let indent = 360 * (lvl as u32 + 1);
        buf.push_str(&format!(
            "{{\\listlevel\\levelnfc0\\levelnfcn0\\leveljc0\\leveljcn0\\levelfollow0\\levelstartat1\
\\levelspace0\\levelindent0{{\\leveltext\\'02\\'0{lvl}.;}}{{\\levelnumbers\\'01;}}\\f0\\fi-360\\li{indent} }}\n"
        ));
    }
    buf.push_str("\\listid1}\n");
    // Unordered: 9 levels of bullet
    buf.push_str("{\\list\\listtemplateid2\\listhybrid\n");
    for lvl in 0u8..9 {
        let indent = 360 * (lvl as u32 + 1);
        buf.push_str(&format!(
            "{{\\listlevel\\levelnfc23\\levelnfcn23\\leveljc0\\leveljcn0\\levelfollow0\\levelstartat1\
\\levelspace0\\levelindent0{{\\leveltext\\'01\\u8226 ?;}}{{\\levelnumbers;}}\\f2\\fi-360\\li{indent} }}\n"
        ));
    }
    buf.push_str("\\listid2}\n");
    buf.push_str("}\n"); // end listtable

    // List override table
    buf.push_str("{\\listoverridetable\n");
    buf.push_str("{\\listoverride\\listid1\\listoverridecount0\\ls1}\n");
    buf.push_str("{\\listoverride\\listid2\\listoverridecount0\\ls2}\n");
    buf.push_str("}\n");

    // Info block
    if !config.title.is_empty() || !config.author.is_empty() {
        buf.push_str("{\\info");
        if !config.title.is_empty() {
            buf.push_str(&format!("{{\\title {}}}", rtf_escape(&config.title)));
        }
        if !config.author.is_empty() {
            buf.push_str(&format!("{{\\author {}}}", rtf_escape(&config.author)));
        }
        buf.push_str("}\n");
    }

    // Page geometry (US Letter, 1-inch margins)
    buf.push_str("\\paperw12240\\paperh15840\\margl1440\\margr1440\\margt1440\\margb1440\n");
    buf.push_str("\\widowctrl\\hyphauto\\ftnbj\\enddoc\n");
    // Section definition
    buf.push_str("\\sectd\\pgwsxn12240\\pghsxn15840\\marglsxn1440\\margrsxn1440\n");
    buf.push_str("\\headery720\\footery720\\pgndec\n");
    buf.push_str("\\f0\\fs24\\cf1\n");
}

// ── Pre-scan: collect footnote definitions ────────────────────────────────────

fn collect_footnotes<'a>(events: &[Event<'a>]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut in_def = false;
    let mut label = String::new();
    let mut body = String::new();
    for ev in events {
        match ev {
            Event::Start(Tag::FootnoteDefinition(l)) => {
                in_def = true;
                label = l.to_string();
                body.clear();
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                map.insert(std::mem::take(&mut label), std::mem::take(&mut body));
                in_def = false;
            }
            Event::Text(t) if in_def => body.push_str(t),
            _ => {}
        }
    }
    map
}

// ── Writer state ──────────────────────────────────────────────────────────────

struct WriterState {
    in_paragraph: bool,
    list_stack: Vec<bool>, // true=ordered
    list_counters: Vec<u32>,
    in_blockquote: bool,
    in_code_block: bool,
    open_groups: u32,
    // Table buffering
    in_table: bool,
    cell_sink: Option<String>,
    current_row: Vec<String>,
    table_rows: Vec<(bool, Vec<String>)>, // (is_header, cells)
    // Footnotes
    footnote_defs: HashMap<String, String>,
    footnote_counter: u32,
    footnote_entries: Vec<(u32, String)>, // (num, body)
}

impl WriterState {
    fn new(footnotes: HashMap<String, String>) -> Self {
        Self {
            in_paragraph: false,
            list_stack: vec![],
            list_counters: vec![],
            in_blockquote: false,
            in_code_block: false,
            open_groups: 0,
            in_table: false,
            cell_sink: None,
            current_row: vec![],
            table_rows: vec![],
            footnote_defs: footnotes,
            footnote_counter: 0,
            footnote_entries: vec![],
        }
    }
}

// Direct-write helper: redirect to cell_sink when inside a table cell.
#[inline]
fn push(s: &mut WriterState, buf: &mut String, text: &str) {
    match s.cell_sink {
        Some(ref mut sink) => sink.push_str(text),
        None => buf.push_str(text),
    }
}

// ── Event dispatch ────────────────────────────────────────────────────────────

fn write_event(buf: &mut String, ev: &Event<'_>, s: &mut WriterState) {
    match ev {
        // Paragraph
        Event::Start(Tag::Paragraph) if !s.in_table => {
            s.in_paragraph = true;
            let li = if s.in_blockquote {
                "\\li720\\ri720 "
            } else {
                ""
            };
            buf.push_str(&format!("\\pard\\s0{}\\widctlpar\\f0\\fs24\\cf1 ", li));
        }
        Event::End(TagEnd::Paragraph) if !s.in_table => {
            close_groups(buf, &mut s.open_groups);
            buf.push_str("\\par\n");
            s.in_paragraph = false;
        }

        // Headings
        Event::Start(Tag::Heading { level, .. }) => {
            let (sn, sz) = heading_params(*level);
            let li = if s.in_blockquote {
                "\\li720\\ri720 "
            } else {
                ""
            };
            buf.push_str(&format!(
                "\\pard\\s{sn}\\sb240\\sa60\\keepn{}\\widctlpar\\b\\fs{sz}\\cf1 ",
                li
            ));
        }
        Event::End(TagEnd::Heading(_)) => {
            close_groups(buf, &mut s.open_groups);
            buf.push_str("\\b0\\par\n");
        }

        // Blockquote
        Event::Start(Tag::BlockQuote(_)) => {
            s.in_blockquote = true;
        }
        Event::End(TagEnd::BlockQuote(_)) => {
            s.in_blockquote = false;
            close_groups(buf, &mut s.open_groups);
            buf.push_str("\\pard\\par\n");
        }

        // Code blocks
        Event::Start(Tag::CodeBlock(kind)) => {
            s.in_code_block = true;
            let lang = if let CodeBlockKind::Fenced(l) = kind {
                l.as_ref()
            } else {
                ""
            };
            buf.push_str("\\pard\\widctlpar\\f1\\fs20\\cf2\\li360\\ri360 ");
            if !lang.is_empty() {
                buf.push_str(&format!("{{\\i {}}}\\line\n", rtf_escape(lang)));
            }
        }
        Event::End(TagEnd::CodeBlock) => {
            s.in_code_block = false;
            buf.push_str("\\par\\pard\\f0\\fs24\\cf1\n");
        }

        // Lists
        Event::Start(Tag::List(first)) => {
            s.list_stack.push(first.is_some());
            s.list_counters.push(first.unwrap_or(1) as u32);
        }
        Event::End(TagEnd::List(_)) => {
            s.list_stack.pop();
            s.list_counters.pop();
            buf.push_str("\\pard\\par\n");
        }
        Event::Start(Tag::Item) => {
            let depth = (s.list_stack.len() as u32).saturating_sub(1);
            let indent = 360 * (depth + 1);
            let ordered = s.list_stack.last().copied().unwrap_or(false);
            if ordered {
                let n = s
                    .list_counters
                    .last_mut()
                    .map(|c| {
                        *c += 1;
                        *c - 1
                    })
                    .unwrap_or(1);
                buf.push_str(&format!(
                    "\\pard\\ls1\\ilvl{depth}\\fi-360\\li{indent}\\widctlpar\\f0\\fs24\\cf1 {}. ",
                    n
                ));
            } else {
                buf.push_str(&format!("\\pard\\ls2\\ilvl{depth}\\fi-360\\li{indent}\\widctlpar\\f0\\fs24\\cf1 \\bullet  "));
            }
        }
        Event::End(TagEnd::Item) => {
            close_groups(buf, &mut s.open_groups);
            buf.push_str("\\par\n");
        }

        // Tables — fully buffered until End(Table)
        Event::Start(Tag::Table(_)) => {
            s.in_table = true;
            s.table_rows.clear();
        }
        Event::End(TagEnd::Table) => {
            flush_table(buf, s);
            s.in_table = false;
        }
        Event::Start(Tag::TableHead) => {
            s.current_row.clear();
        }
        Event::End(TagEnd::TableHead) => {
            let row = std::mem::take(&mut s.current_row);
            s.table_rows.push((true, row));
        }
        Event::Start(Tag::TableRow) => {
            s.current_row.clear();
        }
        Event::End(TagEnd::TableRow) => {
            let row = std::mem::take(&mut s.current_row);
            s.table_rows.push((false, row));
        }
        Event::Start(Tag::TableCell) => {
            s.cell_sink = Some(String::new());
        }
        Event::End(TagEnd::TableCell) => {
            close_groups_in_sink(s);
            let cell = s.cell_sink.take().unwrap_or_default();
            s.current_row.push(cell);
        }

        Event::Rule => {
            buf.push_str("\\pard\\brdrb\\brdrs\\brdrw10\\brdr0\\par\\pard\n");
        }

        // Inline formatting
        Event::Start(Tag::Strong) => {
            push(s, buf, "{\\b ");
            s.open_groups += 1;
        }
        Event::End(TagEnd::Strong) if s.open_groups > 0 => {
            push(s, buf, "}");
            s.open_groups -= 1;
        }
        Event::Start(Tag::Emphasis) => {
            push(s, buf, "{\\i ");
            s.open_groups += 1;
        }
        Event::End(TagEnd::Emphasis) if s.open_groups > 0 => {
            push(s, buf, "}");
            s.open_groups -= 1;
        }
        Event::Start(Tag::Strikethrough) => {
            push(s, buf, "{\\strike ");
            s.open_groups += 1;
        }
        Event::End(TagEnd::Strikethrough) if s.open_groups > 0 => {
            push(s, buf, "}");
            s.open_groups -= 1;
        }
        Event::Code(c) => {
            push(s, buf, &format!("{{\\f1\\fs20\\cf2 {}}}", rtf_escape(c)));
        }

        // Hyperlinks
        Event::Start(Tag::Link { dest_url, .. }) => {
            let escaped = rtf_escape(dest_url);
            push(
                s,
                buf,
                &format!("{{\\field{{\\*\\fldinst HYPERLINK \"{escaped}\"}}{{\\fldrslt\\cf3\\ul "),
            );
            s.open_groups += 1;
        }
        Event::End(TagEnd::Link) if s.open_groups > 0 => {
            push(s, buf, "}}");
            s.open_groups -= 1;
        }

        // Images
        Event::Start(Tag::Image {
            dest_url, title, ..
        }) => {
            if dest_url.starts_with("data:image/") {
                let rtf_pict = embed_data_image(dest_url);
                push(s, buf, &rtf_pict);
            } else {
                let alt = if title.is_empty() {
                    dest_url.as_ref()
                } else {
                    title.as_ref()
                };
                push(s, buf, &format!("[Figure: {}]", rtf_escape(alt)));
            }
        }
        Event::End(TagEnd::Image) => {}

        // Math
        Event::InlineMath(m) => {
            let esc = rtf_escape(m);
            push(
                s,
                buf,
                &format!("{{\\*\\momath ${}$}}{{\\f1\\fs20 ${}$}}", esc, esc),
            );
        }
        Event::DisplayMath(m) => {
            let esc = rtf_escape(m);
            buf.push_str(&format!(
                "{{\\*\\momath $${}$$}}\\pard\\f1\\fs20\\li360\\cf2 $${}$$\\par\\pard\\f0\\fs24\\cf1\n",
                esc, esc
            ));
        }

        // Text
        Event::Text(t) => {
            if s.in_code_block {
                for line in t.split('\n') {
                    if !line.is_empty() {
                        buf.push_str(&rtf_escape(line));
                    }
                    buf.push_str("\\line\n");
                }
            } else {
                push(s, buf, &rtf_escape(t));
            }
        }
        Event::SoftBreak => push(s, buf, " "),
        Event::HardBreak => push(s, buf, "\\line\n"),
        Event::Html(h) => push(s, buf, &rtf_escape(&strip_tags(h))),

        // Footnotes
        Event::FootnoteReference(label) => {
            s.footnote_counter += 1;
            let n = s.footnote_counter;
            let body = s
                .footnote_defs
                .get(label.as_ref())
                .cloned()
                .unwrap_or_default();
            s.footnote_entries.push((n, body));
            push(s, buf, &format!("{{\\super [{}]}}", n));
        }
        Event::Start(Tag::FootnoteDefinition(_)) | Event::End(TagEnd::FootnoteDefinition) => {}

        _ => {}
    }
}

// ── Table emission ────────────────────────────────────────────────────────────

fn flush_table(buf: &mut String, s: &mut WriterState) {
    if s.table_rows.is_empty() {
        return;
    }
    let col_count = s
        .table_rows
        .iter()
        .map(|(_, r)| r.len())
        .max()
        .unwrap_or(1)
        .max(1);
    let col_w = BODY_W / col_count as u32;
    let cell_borders = "\\clbrdrt\\brdrs\\brdrw10\\clbrdrl\\brdrs\\brdrw10\\clbrdrb\\brdrs\\brdrw10\\clbrdrr\\brdrs\\brdrw10";

    for (is_header, cells) in std::mem::take(&mut s.table_rows) {
        // Row definition
        buf.push_str("\\trowd\\trgaph108\\trleft0 ");
        for c in 1..=col_count {
            buf.push_str(cell_borders);
            buf.push_str(&format!("\\cellx{} ", col_w * c as u32));
        }
        buf.push_str("\\pard\\intbl\\widctlpar");
        if is_header {
            buf.push_str("\\b");
        }
        buf.push(' ');

        // Cell content
        let mut padded = cells;
        padded.resize(col_count, String::new());
        for cell in padded {
            buf.push_str(&cell);
            buf.push_str("\\cell ");
        }
        if is_header {
            buf.push_str("\\b0");
        }
        buf.push_str("\\row\n");
    }
    buf.push_str("\\pard\\widctlpar\\par\n");
}

fn close_groups_in_sink(s: &mut WriterState) {
    if s.open_groups > 0 {
        if let Some(ref mut sink) = s.cell_sink {
            for _ in 0..s.open_groups {
                sink.push('}');
            }
        }
        s.open_groups = 0;
    }
}

// ── Footnote section ──────────────────────────────────────────────────────────

fn emit_footnote_section(buf: &mut String, s: &WriterState) {
    buf.push_str("\\pard\\brdrb\\brdrs\\brdrw10\\brdr0\\par\\pard\n");
    buf.push_str("\\pard\\widctlpar\\b\\fs20 Footnotes\\b0\\par\n");
    for (n, body) in &s.footnote_entries {
        buf.push_str(&format!(
            "\\pard\\widctlpar\\f0\\fs20 [{}] {}\\par\n",
            n,
            rtf_escape(body)
        ));
    }
}

// ── Utility ───────────────────────────────────────────────────────────────────

fn heading_params(level: HeadingLevel) -> (u8, u32) {
    match level {
        HeadingLevel::H1 => (1, 48),
        HeadingLevel::H2 => (2, 40),
        HeadingLevel::H3 => (3, 32),
        HeadingLevel::H4 => (4, 28),
        HeadingLevel::H5 => (5, 24),
        HeadingLevel::H6 => (6, 20),
    }
}

fn close_groups(buf: &mut String, count: &mut u32) {
    for _ in 0..*count {
        buf.push('}');
    }
    *count = 0;
}

/// Embed a data-URI image as an RTF \pict\pngblip hex block.
/// Supports `data:image/png;base64,...` and `data:image/jpeg;base64,...`.
fn embed_data_image(data_uri: &str) -> String {
    use base64::Engine as _;
    let enc = base64::engine::general_purpose::STANDARD;

    let is_png = data_uri.starts_with("data:image/png");
    let is_jpeg = data_uri.starts_with("data:image/jpeg") || data_uri.starts_with("data:image/jpg");
    if !is_png && !is_jpeg {
        return "[Image]".to_string();
    }

    let b64 = match data_uri.find(',') {
        Some(i) => &data_uri[i + 1..],
        None => return "[Image]".to_string(),
    };
    let bytes = match enc.decode(b64.trim()) {
        Ok(b) => b,
        Err(_) => return "[Image]".to_string(),
    };

    // Determine pixel dimensions (best-effort; defaults to 72dpi)
    let (pw, ph) = image_dimensions(&bytes, is_png);
    let pict_type = if is_png { "\\pngblip" } else { "\\jpegblip" };

    // Convert pixels to twips (72 DPI assumed: 1 twip = 1/1440 in; pixel = 1/72 in → 20 twips/px)
    let picw = pw * 20;
    let pich = ph * 20;
    // Scale to fit body width
    let (scalex, scaley) = if picw > BODY_W {
        let scale = (BODY_W * 100) / picw.max(1);
        (scale, scale)
    } else {
        (100, 100)
    };

    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    format!(
        "{{\\pict{pict_type}\\picw{picw}\\pich{pich}\\picscalex{scalex}\\picscaley{scaley} {hex}}}"
    )
}

/// Read width/height from PNG (IHDR) or JPEG (SOF0/2) header bytes.
fn image_dimensions(bytes: &[u8], is_png: bool) -> (u32, u32) {
    if is_png && bytes.len() >= 24 {
        // PNG IHDR: bytes 16-19 = width, 20-23 = height
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return (w.max(1), h.max(1));
    }
    if !is_png {
        // JPEG: scan for SOFn markers
        let mut i = 0usize;
        while i + 3 < bytes.len() {
            if bytes[i] == 0xFF && matches!(bytes[i + 1], 0xC0..=0xC2) && i + 8 < bytes.len() {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return (w.max(1), h.max(1));
            }
            i += 1;
        }
    }
    (100, 100)
}

/// Escape a UTF-8 string for RTF: `{`, `}`, `\` → escaped; non-ASCII → `\uN?`.
pub fn rtf_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\line\n"),
            '\r' => {}
            '\t' => out.push_str("\\tab "),
            c if c.is_ascii() => out.push(c),
            c => out.push_str(&format!("\\u{}?", c as i32)),
        }
    }
    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
            }
            '>' => {
                in_tag = false;
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}
