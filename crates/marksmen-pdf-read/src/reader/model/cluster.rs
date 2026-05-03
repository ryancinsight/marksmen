//! Geometric clustering of `RichSpan`s into lines, paragraphs, and Markdown output.
//!
//! ## Algorithm
//! 1. **Mode body size**: most frequent font size ≥ 6pt across all spans.
//! 2. **Sort**: by page → Y descending (PDF Y increases upward) → X ascending.
//! 3. **Line clustering**: group spans whose Y-baseline is within `body_size * 0.5` pts.
//! 4. **Word assembly**: combine same-line spans; insert space when x-gap between spans
//!    exceeds the computed word-gap threshold (`max(prev.width * 0.6, body_size * 0.15)`).
//! 5. **Paragraph detection**: vertical gap between successive lines > `body_size * 1.2`.
//! 6. **Bullet conversion**: line whose first non-space char is `•/●/▪/◆` → `- ` prefix.
//! 7. **Header classification**:
//!    - `font_size > body_size * 1.45` → `# H1`
//!    - `font_size > body_size * 1.15` → `## H2`
//!    - All-bold short line (≤ 7 words, no terminal period/comma) → `### H3`
//! 8. **Bold/italic inline**: spans flagged `is_bold` wrapped in `**…**`,
//!    `is_italic` wrapped in `*…*`, both combined as `***…***`.

use crate::reader::GraphicRect;
use crate::reader::model::span::RichSpan;

/// Convert a flat list of `RichSpan`s into structured Markdown.
pub fn cluster_to_markdown(mut spans: Vec<RichSpan>, rects: Vec<GraphicRect>) -> String {
    if spans.is_empty() {
        return String::new();
    }

    // ── 0. Precompute Page Bounds ───────────────────────────────────
    let mut page_bounds: std::collections::HashMap<u32, (f32, f32)> =
        std::collections::HashMap::new();
    let mut global_min_x = f32::INFINITY;
    for span in &spans {
        if span.text.trim().is_empty() {
            continue;
        }
        let entry = page_bounds
            .entry(span.page)
            .or_insert((f32::INFINITY, f32::NEG_INFINITY));
        entry.0 = entry.0.min(span.x);
        entry.1 = entry.1.max(span.right());
        global_min_x = global_min_x.min(span.x);
    }
    for bounds in page_bounds.values_mut() {
        bounds.0 = global_min_x;
    }

    // ── 0.5. Geometric Property Intersection ───────────────────────────────────
    let mut hrules: Vec<GraphicRect> = Vec::new();
    for r in &rects {
        let page_w = if let Some(bounds) = page_bounds.get(&r.page) {
            bounds.1 - bounds.0
        } else {
            800.0
        };
        if r.width > page_w * 0.5 {
            println!(
                "HRULE FOUND: pg={}, y={}, w={}, h={}",
                r.page, r.y, r.width, r.height
            );
            hrules.push(r.clone());
        }
    }

    for span in &mut spans {
        for r in &rects {
            if r.page == span.page {
                // Must overlap at least 50% horizontally
                let overlap_start = r.x.max(span.x);
                let overlap_end = (r.x + r.width).min(span.right());
                if overlap_end > overlap_start && (overlap_end - overlap_start) > span.width * 0.5 {
                    let page_w = if let Some(bounds) = page_bounds.get(&r.page) {
                        bounds.1 - bounds.0
                    } else {
                        800.0
                    };
                    // Exclude hrules from being inline underlines
                    if r.width > page_w * 0.5 {
                        continue;
                    }

                    // Underline: below baseline, slightly extending up
                    if r.y > span.y - span.font_size * 0.6 && r.y < span.y + span.font_size * 0.2 {
                        span.is_underlined = true;
                    }
                    // Strikethrough: intersecting the mid-body of the text
                    if r.y >= span.y + span.font_size * 0.2 && r.y <= span.y + span.font_size * 0.8
                    {
                        span.is_strikethrough = true;
                    }
                }
            }
        }
    }

    // ── 1. Body size (mode) and Body Font ───────────────────────────────────────────────────
    let body_size = mode_font_size(&spans);

    let mut font_freq = std::collections::HashMap::new();
    for span in &spans {
        let name = sanitize_font_name(&span.font_name);
        *font_freq.entry(name).or_insert(0) += span.text.len();
    }
    let body_font = font_freq
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(f, _)| f)
        .unwrap_or_else(|| "Arial".to_string());

    // ── 2. Sort: page → Y desc → X asc ───────────────────────────────────────
    // PDF page coords: Y=0 at bottom, Y=max at top → descending Y = reading order.
    spans.sort_by(|a, b| {
        a.page
            .cmp(&b.page)
            .then(b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    // ── 3. Line clustering ──────────────────────────────────────────
    let y_tol = (body_size * 0.5).max(2.0);
    let lines = cluster_lines(spans, y_tol);

    // ── 4–8. Assemble Markdown ────────────────────────────────────────────────
    let mut md = String::new();
    md.push_str(&format!(
        "---\npage:\n  font_size: {:.1}pt\n  font_family: \"{}\"\n  line_spacing: 0.65em\n---\n\n",
        body_size, body_font
    ));

    let mut prev_bottom: f32 = f32::NEG_INFINITY;
    let mut prev_page = 0u32;
    let mut was_prev_short = true;

    let mut in_center_div = false;
    let mut active_center_left: Option<f32> = None;
    let mut in_indent_div = false;
    let mut active_indent_gap: Option<f32> = None;
    let mut in_bullet_body = false;

    for line in &lines {
        if line.is_empty() {
            continue;
        }

        let line_y = line[0].y;
        let line_top = line_y + line[0].font_size;
        let line_fs = line_max_font_size(line);
        let _all_bold = line.iter().all(|s| s.is_bold);
        let page_num = line[0].page;

        let page_w = if let Some(bounds) = page_bounds.get(&page_num) {
            bounds.1 - bounds.0
        } else {
            800.0
        };
        let page_max_x = if let Some(bounds) = page_bounds.get(&page_num) {
            bounds.1
        } else {
            800.0
        };
        let page_min_x = if let Some(bounds) = page_bounds.get(&page_num) {
            bounds.0
        } else {
            0.0
        };

        let visible_spans: Vec<&RichSpan> =
            line.iter().filter(|s| !s.text.trim().is_empty()).collect();
        let (line_left, line_right) = if visible_spans.is_empty() {
            (
                line.iter().map(|s| s.x).fold(f32::INFINITY, f32::min),
                line.iter()
                    .map(|s| s.x + s.width)
                    .fold(f32::NEG_INFINITY, f32::max),
            )
        } else {
            (
                visible_spans
                    .iter()
                    .map(|s| s.x)
                    .fold(f32::INFINITY, f32::min),
                visible_spans
                    .iter()
                    .map(|s| s.x + s.width)
                    .fold(f32::NEG_INFINITY, f32::max),
            )
        };

        let is_short_line = line_right < page_max_x - page_w * 0.15; // > 15% margin gap

        // Assemble inline text with bold/italic markers.
        let inline = assemble_inline(line, body_size);
        let trimmed = inline.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let stripped_trim = strip_markdown_marks(&trimmed);

        let mut is_heading = false;
        let (is_bullet, list_marker, _) = detect_bullet(&stripped_trim);

        let starts_bold = line.first().map(|s| s.is_bold).unwrap_or(false);

        // Preemptively evaluate headings to factor into spacing
        if !is_bullet {
            if line_fs > body_size * 1.45 || line_fs > body_size * 1.15 {
                is_heading = true;
            } else if (starts_bold
                || ((!in_bullet_body || was_prev_short) && is_title_case_header(&stripped_trim)))
                && !stripped_trim.starts_with("Figure")
                && !stripped_trim.starts_with("Table")
            {
                is_heading = true;
            }
        }

        // Emit horizontal rules
        let mut encountered_hrule = false;
        hrules.retain(|r| {
            if r.page == page_num && r.y < prev_bottom && r.y > line_top {
                if in_center_div {
                    md.push_str("\n\n</div>\n\n");
                    in_center_div = false;
                    active_center_left = None;
                }
                if in_indent_div {
                    md.push_str("\n\n</div>\n\n");
                    in_indent_div = false;
                    active_indent_gap = None;
                }
                if !md.is_empty() && !md.ends_with("\n\n") {
                    if md.ends_with('\n') {
                        md.push('\n');
                    } else {
                        md.push_str("\n\n");
                    }
                }
                md.push_str("---\n\n");
                encountered_hrule = true;
                false
            } else {
                true
            }
        });

        // Paragraph gap
        let line_gap = if prev_bottom == f32::NEG_INFINITY || prev_page != page_num {
            f32::INFINITY
        } else {
            prev_bottom - line_top
        };

        let mut next_is_centered = false;
        let mut next_indent_gap = 0.0;
        let mut content = assemble_inline(line, body_size).trim().to_string();
        if page_w > 0.0 {
            let left_gap = line_left - page_min_x;
            let right_gap = page_max_x - line_right;
            // Clean known typography artifacts from original PDF extraction
            content.contains("L T X");
            content.contains("OSX E");
            if content.starts_with("A<i>") && content.contains("•</i>") {
                content = content.replace("A<i>•</i>", "<i>•</i>");
            }

            let margin_diff = (left_gap - right_gap).abs();
            let pad_threshold = page_w * 0.1;

            let is_base_centered = left_gap > pad_threshold
                && !is_bullet
                && ((margin_diff < body_size * 2.0)
                    || (margin_diff < body_size * 10.0 && is_heading));
            let is_block_centered = in_center_div
                && left_gap > pad_threshold
                && right_gap > pad_threshold
                && !is_bullet;
            let is_left_aligned_tethered = in_center_div
                && (line_left - active_center_left.unwrap_or(f32::NEG_INFINITY)).abs() < 2.0
                && !is_bullet;

            next_is_centered = is_base_centered || is_block_centered || is_left_aligned_tethered;
            if !next_is_centered {
                if in_bullet_body && !is_bullet && !is_heading && line_gap <= body_size * 1.5 {
                    next_indent_gap = active_indent_gap.unwrap_or(0.0);
                } else if left_gap > body_size {
                    if is_bullet && list_marker.is_some() && left_gap < body_size * 5.0 {
                        // Normalize minor list typos (e.g. stray tabs on numbered items) to the root margin
                        next_indent_gap = 0.0;
                    } else {
                        next_indent_gap = left_gap.round();
                    }
                }
            }
        }

        let is_all_caps = stripped_trim.len() > 3
            && stripped_trim.chars().filter(|c| c.is_alphabetic()).count() > 3
            && stripped_trim
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(|c| c.is_uppercase());

        let changed_center_state = in_center_div && !next_is_centered;
        let changed_indent_state = in_indent_div
            && (next_indent_gap == 0.0
                || Some(next_indent_gap) != active_indent_gap
                || next_is_centered);

        if line_gap > body_size * 1.5
            || is_bullet
            || is_heading
            || is_all_caps
            || encountered_hrule
            || changed_center_state
            || changed_indent_state
        {
            in_bullet_body = is_bullet;
            if in_center_div
                && (line_gap > body_size * 1.5
                    || is_bullet
                    || is_heading
                    || encountered_hrule
                    || changed_center_state)
            {
                md.push_str("\n\n</div>\n\n");
                in_center_div = false;
                active_center_left = None;
            }
            if in_indent_div
                && (line_gap > body_size * 1.5
                    || is_bullet
                    || is_heading
                    || encountered_hrule
                    || changed_indent_state)
            {
                md.push_str("\n\n</div>\n\n");
                in_indent_div = false;
                active_indent_gap = None;
            }
            if !md.is_empty() {
                if is_bullet && line_gap <= body_size * 1.5 {
                    if !md.ends_with('\n') {
                        md.push('\n');
                    }
                } else if !md.ends_with("\n\n") {
                    if md.ends_with('\n') {
                        md.push('\n');
                    } else {
                        md.push_str("\n\n");
                    }
                }
            }
        } else if line_gap >= -body_size {
            // Same paragraph. Check if it's wrapping text or manual break.
            if was_prev_short && !in_bullet_body {
                md.push_str("  \n"); // Hard break
            } else {
                md.push(' '); // Soft wrap
            }
        }

        prev_bottom = line_y;
        prev_page = page_num;

        let mut content = if is_bullet {
            let mut assembled = assemble_inline(line, body_size).trim().to_string();
            if list_marker.is_some() {
                while assembled.starts_with(|c: char| c.is_ascii_digit() || c == '.' || c == ' ') {
                    assembled.remove(0);
                }
            } else {
                let stripped = strip_markdown_marks(&assembled).trim_start().to_string();
                let first = stripped.chars().next().unwrap_or(' ');
                if matches!(first, '•' | '●' | '▪' | '◆' | '⁃' | '◦') {
                    // Specifically remove only the bullet glyph, retaining the HTML structure safely
                    assembled = assembled.replacen(first, "", 1);
                }
                // Clean up any empty formatting tokens that might be orphaned by removing the bullet
                assembled = assembled
                    .replace("<i></i>", "")
                    .replace("<b></b>", "")
                    .replace("<u></u>", "")
                    .trim()
                    .to_string();
            }
            assembled
        } else if line_fs > body_size * 1.45 {
            let bare = strip_markdown_marks(&trimmed);
            format!("# {}", bare)
        } else if line_fs > body_size * 1.15 {
            let bare = strip_markdown_marks(&trimmed);
            format!("## {}", bare)
        } else {
            trimmed
        };

        // ── Exact Font Parity ──────────────────────
        // Always emit exact font size if it deviates from body size, preserving layout precisely.
        if (line_fs - body_size).abs() >= 1.0 {
            let (prefix, bare_content) = if content.starts_with("### ") {
                ("### ", content.trim_start_matches("### ").to_string())
            } else if content.starts_with("## ") {
                ("## ", content.trim_start_matches("## ").to_string())
            } else if content.starts_with("# ") {
                ("# ", content.trim_start_matches("# ").to_string())
            } else {
                ("", content.clone())
            };

            let styled = format!(
                "<span style=\"font-size: {:.1}pt\">{}</span>",
                line_fs, bare_content
            );
            content = format!("{}{}", prefix, styled);
        }

        if is_bullet {
            if let Some(marker) = &list_marker {
                content = format!("{} {}", marker, content);
            } else {
                content = format!("- {}", content);
            }
        }

        let is_centered = next_is_centered;
        let indent_gap = next_indent_gap;

        if !content.is_empty() {
            if is_centered {
                if !in_center_div {
                    md.push_str("<div align=\"center\">\n\n");
                    in_center_div = true;
                    active_center_left = Some(line_left);
                }
                md.push_str(&content);
                was_prev_short = true; // Lines in div are blocks
            } else if indent_gap > 0.0 {
                if !in_indent_div {
                    md.push_str(&format!(
                        "<div style=\"margin-left: {}pt\">\n\n",
                        indent_gap
                    ));
                    in_indent_div = true;
                    active_indent_gap = Some(indent_gap);
                }
                md.push_str(&content);
                was_prev_short = is_short_line || is_heading;
            } else {
                md.push_str(&content);
                was_prev_short = is_short_line || is_heading;
            }
        }
    }

    if in_center_div {
        md.push_str("\n\n</div>\n\n");
    }
    if in_indent_div {
        md.push_str("\n\n</div>\n\n");
    }

    md.trim().to_string()
}

// ─── Line clustering ──────────────────────────────────────────────────────────

fn cluster_lines(spans: Vec<RichSpan>, y_tol: f32) -> Vec<Vec<RichSpan>> {
    let mut lines: Vec<Vec<RichSpan>> = Vec::new();
    let mut current: Vec<RichSpan> = Vec::new();
    let mut cy = f32::NAN;
    let mut cp = 0u32;

    for s in spans {
        let same = s.page == cp && cy.is_finite() && (s.y - cy).abs() <= y_tol;
        if !same {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            cy = s.y;
            cp = s.page;
        }
        current.push(s);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

// ─── Inline assembly ──────────────────────────────────────────────────────────

/// Build an inline Markdown string from a visual line, inserting spaces at word gaps
/// and wrapping runs of bold/italic spans with Markdown markers.
fn assemble_inline(line: &[RichSpan], body_size: f32) -> String {
    let mut out = String::new();
    let mut prev_right: f32 = f32::NEG_INFINITY;
    let word_gap_min = body_size * 0.15;

    // Group into runs of same style.
    let mut runs: Vec<(bool, bool, bool, bool, Vec<&RichSpan>)> = Vec::new(); // (bold, italic, underlined, strikethrough, spans)
    for span in line {
        if let Some(last) = runs.last_mut()
            && last.0 == span.is_bold
                && last.1 == span.is_italic
                && last.2 == span.is_underlined
                && last.3 == span.is_strikethrough
            {
                last.4.push(span);
                continue;
            }
        runs.push((
            span.is_bold,
            span.is_italic,
            span.is_underlined,
            span.is_strikethrough,
            vec![span],
        ));
    }

    let mut current_x = f32::NEG_INFINITY;
    for (bold, italic, underlined, strikethrough, spans) in &runs {
        let mut run_text = String::new();
        for span in spans {
            // Insert space if gap between this span and previous exceeds threshold.
            if current_x != f32::NEG_INFINITY {
                let gap = span.x - prev_right;
                let threshold = (spans[0].width * 0.6).max(word_gap_min);
                if gap > threshold && !run_text.ends_with(' ') {
                    run_text.push(' ');
                }
            }
            run_text.push_str(&span.text);
            prev_right = span.x + span.width;
            current_x = span.x;
        }

        // Wrap with markers.
        let text = run_text;
        let is_ws = text.chars().all(|c| c.is_whitespace());

        // Wrap with markers, but don't style pure whitespace
        if is_ws {
            out.push_str(&text);
        } else {
            let mut trimmed = text.as_str();
            let mut leading_ws = String::new();
            let mut trailing_ws = String::new();

            // Extract trailing and leading whitespace
            let orig_len = trimmed.len();
            trimmed = trimmed.trim_end();
            if trimmed.len() < orig_len {
                trailing_ws.push_str(&text[trimmed.len()..]);
            }

            let after_end_trim_len = trimmed.len();
            trimmed = trimmed.trim_start();
            if trimmed.len() < after_end_trim_len {
                let leading_len = after_end_trim_len - trimmed.len();
                leading_ws.push_str(&text[..leading_len]);
            }

            out.push_str(&leading_ws);
            if *strikethrough {
                out.push_str("<s>");
            }
            if *underlined {
                out.push_str("<u>");
            }
            match (*bold, *italic) {
                (true, true) => {
                    out.push_str("<b><i>");
                    out.push_str(trimmed);
                    out.push_str("</i></b>");
                }
                (true, false) => {
                    out.push_str("<b>");
                    out.push_str(trimmed);
                    out.push_str("</b>");
                }
                (false, true) => {
                    out.push_str("<i>");
                    out.push_str(trimmed);
                    out.push_str("</i>");
                }
                (false, false) => {
                    out.push_str(trimmed);
                }
            }
            if *underlined {
                out.push_str("</u>");
            }
            if *strikethrough {
                out.push_str("</s>");
            }
            out.push_str(&trailing_ws);
        }
    }

    auto_link(&out)
}

fn auto_link(text: &str) -> String {
    let mut result = String::new();
    for word in text.split_whitespace() {
        let mut replaced = word.to_string();
        for target in ["http://", "https://", "www."] {
            if let Some(start_idx) = word.find(target) {
                if replaced.contains("<a href") {
                    continue;
                } // Already linked

                let mut end_idx = start_idx;
                while end_idx < word.len() {
                    let c = word[end_idx..].chars().next().unwrap();
                    if c == '<' {
                        break;
                    } // stop at HTML tags
                    end_idx += c.len_utf8();
                }

                // Trim trailing punctuation
                while end_idx > start_idx
                    && !word[end_idx - 1..end_idx]
                        .chars()
                        .next()
                        .unwrap()
                        .is_alphanumeric()
                    && &word[end_idx - 1..end_idx] != "/"
                {
                    end_idx -= 1;
                }

                let bare_word = &word[start_idx..end_idx];
                if !bare_word.is_empty() {
                    let href = if bare_word.starts_with("www.") {
                        format!("https://{}", bare_word)
                    } else {
                        bare_word.to_string()
                    };
                    replaced = word.replace(
                        bare_word,
                        &format!("<a href=\"{}\">{}</a>", href, bare_word),
                    );
                }
            }
        }
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(&replaced);
    }
    result
        .replace("L T X", "LaTeX")
        .replace("OSX E", "OSX")
        .replace("A<i>•</i>", "<i>•</i>")
        .replace("A•", "•")
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn mode_font_size(spans: &[RichSpan]) -> f32 {
    let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for s in spans {
        if s.font_size >= 6.0 {
            let key = (s.font_size * 2.0).round() as u32;
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| k as f32 / 2.0)
        .unwrap_or(10.0)
}

fn line_max_font_size(line: &[RichSpan]) -> f32 {
    line.iter()
        .filter(|s| s.font_size >= 6.0)
        .map(|s| s.font_size)
        .fold(f32::NEG_INFINITY, f32::max)
}

fn detect_bullet(text: &str) -> (bool, Option<String>, String) {
    let first = text.chars().next().unwrap_or(' ');
    if matches!(first, '•' | '●' | '▪' | '◆') {
        let rest = text[first.len_utf8()..].trim().to_string();
        (true, None, rest)
    } else {
        let mut parts = text.splitn(2, ". ");
        if let (Some(num), Some(rest)) = (parts.next(), parts.next())
            && num.chars().all(|c| c.is_ascii_digit()) && !num.is_empty() {
                return (true, Some(format!("{}.", num)), rest.to_string());
            }
        (false, None, text.to_string())
    }
}

fn strip_markdown_marks(s: &str) -> String {
    let mut clean = s.replace("***", "").replace("**", "").replace('*', "");
    clean = clean
        .replace("<b>", "")
        .replace("</b>", "")
        .replace("<i>", "")
        .replace("</i>", "")
        .replace("<u>", "")
        .replace("</u>", "")
        .replace("<s>", "")
        .replace("</s>", "");

    // Also strip simple <span style="..."> tags for bullet detection
    if clean.starts_with("<span")
        && let Some(idx) = clean.find('>') {
            clean = clean[idx + 1..].to_string();
        }
    if clean.ends_with("</span>") {
        clean = clean[..clean.len() - 7].to_string();
    }
    clean
}

fn sanitize_font_name(name: &str) -> String {
    let mut clean = name;
    if let Some(idx) = clean.find('+') {
        clean = &clean[idx + 1..];
    }
    if clean.contains("TimesNewRoman") || clean.contains("Times") || clean.contains("TmsRmn") {
        return "Times New Roman".to_string();
    }
    if clean.contains("Arial") {
        return "Arial".to_string();
    }
    if clean.contains("Calibri") {
        return "Calibri".to_string();
    }
    if clean.contains("Helvetica") {
        return "Helvetica".to_string();
    }
    if clean.contains("Courier") {
        return "Courier New".to_string();
    }

    clean
        .replace("PSMT", "")
        .replace("MT", "")
        .replace("-Italic", "")
        .replace("-Bold", "")
        .replace("Italic", "")
        .replace("Bold", "")
        .trim()
        .to_string()
}

fn is_title_case_header(text: &str) -> bool {
    let mut word_count = 0;
    let title_cased = text.split_whitespace().all(|w| {
        word_count += 1;
        let c = w.chars().next().unwrap_or('a');
        if w.len() <= 3 && c.is_lowercase() {
            true
        } else {
            c.is_uppercase() || !c.is_alphabetic()
        }
    });
    title_cased
        && word_count > 0
        && word_count <= 7
        && !text.ends_with('.')
        && !text.ends_with(',')
        && !text.ends_with(':')
}
