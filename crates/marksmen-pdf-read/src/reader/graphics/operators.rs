//! PDF content stream operator dispatcher.
//!
//! Processes `lopdf::content::Operation` sequences, updating the `GraphicsState`
//! and emitting `RichSpan`s for each decoded glyph.  All operators are handled
//! per ISO 32000-1 §8 (graphics) and §9 (text).

use crate::reader::{
    GraphicRect,
    font::Font,
    graphics::state::{GraphicsState, Matrix, identity, matrix_mul, matrix_pt},
    model::span::RichSpan,
};
use lopdf::{Dictionary, Document, Object};

/// Process a slice of PDF operations, appending decoded `RichSpan`s and `GraphicRect`s to output vectors.
///
/// `resources` is the `/Resources` dictionary for the current context (page or XObject).
pub fn process_ops(
    ops: &[lopdf::content::Operation],
    state: &mut GraphicsState,
    resources: &Dictionary,
    doc: &Document,
    page: u32,
    out: &mut Vec<RichSpan>,
    out_rects: &mut Vec<GraphicRect>,
) {
    let mut curr_path: Vec<(f32, f32)> = Vec::new();
    let mut curr_rects: Vec<GraphicRect> = Vec::new();

    // Pre-build font cache: resource key → Font (loaded lazily on Tf).
    for op in ops {
        dispatch(
            op,
            state,
            resources,
            doc,
            page,
            out,
            out_rects,
            &mut curr_path,
            &mut curr_rects,
        );
    }
}

fn dispatch(
    op: &lopdf::content::Operation,
    state: &mut GraphicsState,
    resources: &Dictionary,
    doc: &Document,
    page: u32,
    out: &mut Vec<RichSpan>,
    out_rects: &mut Vec<GraphicRect>,
    curr_path: &mut Vec<(f32, f32)>,
    curr_rects: &mut Vec<GraphicRect>,
) {
    match op.operator.as_str() {
        // ── Graphics state ────────────────────────────────────────────────────
        "w" => {
            if let Some(v) = f32_op(&op.operands, 0) {
                state.line_width = v;
            }
        }
        "q" => state.push(),
        "Q" => state.pop(),
        "cm" => {
            if let Some(m) = parse_matrix(&op.operands) {
                state.ctm = matrix_mul(state.ctm, m);
            }
        }

        // ── Color operators ────────────────────────────────────────────────────
        // Fill: rg (RGB), g (gray), k (CMYK), cs/scn (general)
        "rg" => {
            if let (Some(r), Some(g), Some(b)) = (
                f32_op(&op.operands, 0),
                f32_op(&op.operands, 1),
                f32_op(&op.operands, 2),
            ) {
                state.fill_color = (r, g, b);
            }
        }
        "g" => {
            if let Some(v) = f32_op(&op.operands, 0) {
                state.fill_color = (v, v, v);
            }
        }
        "k" => {
            // CMYK → approximate RGB.
            if let (Some(c), Some(m), Some(y), Some(k)) = (
                f32_op(&op.operands, 0),
                f32_op(&op.operands, 1),
                f32_op(&op.operands, 2),
                f32_op(&op.operands, 3),
            ) {
                state.fill_color = cmyk_to_rgb(c, m, y, k);
            }
        }
        // Stroke: RG (RGB), G (gray), K (CMYK).
        "RG" => {
            if let (Some(r), Some(g), Some(b)) = (
                f32_op(&op.operands, 0),
                f32_op(&op.operands, 1),
                f32_op(&op.operands, 2),
            ) {
                state.stroke_color = (r, g, b);
            }
        }
        "G" => {
            if let Some(v) = f32_op(&op.operands, 0) {
                state.stroke_color = (v, v, v);
            }
        }
        "K" => {
            if let (Some(c), Some(m), Some(y), Some(k)) = (
                f32_op(&op.operands, 0),
                f32_op(&op.operands, 1),
                f32_op(&op.operands, 2),
                f32_op(&op.operands, 3),
            ) {
                state.stroke_color = cmyk_to_rgb(c, m, y, k);
            }
        }

        // ── Text state operators ──────────────────────────────────────────────
        "BT" => {
            state.tm = identity();
            state.tlm = identity();
        }
        "ET" => {}
        "Tc" => {
            if let Some(v) = f32_op(&op.operands, 0) {
                state.char_spacing = v;
            }
        }
        "Tw" => {
            if let Some(v) = f32_op(&op.operands, 0) {
                state.word_spacing = v;
            }
        }
        "Tz" => {
            if let Some(v) = f32_op(&op.operands, 0) {
                state.horiz_scale = v;
            }
        }
        "TL" => {
            if let Some(v) = f32_op(&op.operands, 0) {
                state.leading = v;
            }
        }
        "Tr" => {
            if let Some(v) = f32_op(&op.operands, 0) {
                state.render_mode = v as u8;
            }
        }

        // ── Text matrix operators ─────────────────────────────────────────────
        "Tm" => {
            if let Some(m) = parse_matrix(&op.operands) {
                state.tm = m;
                state.tlm = m;
            }
        }
        "Td" => {
            if let (Some(tx), Some(ty)) = (f32_op(&op.operands, 0), f32_op(&op.operands, 1)) {
                let delta = [1.0f32, 0.0, 0.0, 1.0, tx, ty];
                state.tlm = matrix_mul(delta, state.tlm);
                state.tm = state.tlm;
            }
        }
        "TD" => {
            if let (Some(tx), Some(ty)) = (f32_op(&op.operands, 0), f32_op(&op.operands, 1)) {
                state.leading = -ty;
                let delta = [1.0f32, 0.0, 0.0, 1.0, tx, ty];
                state.tlm = matrix_mul(delta, state.tlm);
                state.tm = state.tlm;
            }
        }
        "T*" => {
            let lead = state.leading;
            let delta = [1.0f32, 0.0, 0.0, 1.0, 0.0, -lead];
            state.tlm = matrix_mul(delta, state.tlm);
            state.tm = state.tlm;
        }

        // ── Font selection ────────────────────────────────────────────────────
        "Tf" => {
            if let Some(fs) = f32_op(&op.operands, 1) {
                state.font_size = fs;
            }
            if let Some(name_bytes) = op.operands.first().and_then(|o| o.as_name().ok()) {
                let key = String::from_utf8_lossy(name_bytes).into_owned();
                state.font_resource_name = key.clone();
                state.font = resolve_font(&key, resources, doc);
            }
        }

        // ── Text showing operators ─────────────────────────────────────────────
        "Tj" => {
            if let Some(bytes) = string_bytes(&op.operands, 0) {
                show_string(&bytes, state, page, out);
            }
        }
        "'" => {
            // Move to next line then show.
            let lead = state.leading;
            let delta = [1.0f32, 0.0, 0.0, 1.0, 0.0, -lead];
            state.tlm = matrix_mul(delta, state.tlm);
            state.tm = state.tlm;
            if let Some(bytes) = string_bytes(&op.operands, 0) {
                show_string(&bytes, state, page, out);
            }
        }
        "\"" => {
            if let Some(aw) = f32_op(&op.operands, 0) {
                state.word_spacing = aw;
            }
            if let Some(ac) = f32_op(&op.operands, 1) {
                state.char_spacing = ac;
            }
            let lead = state.leading;
            let delta = [1.0f32, 0.0, 0.0, 1.0, 0.0, -lead];
            state.tlm = matrix_mul(delta, state.tlm);
            state.tm = state.tlm;
            if let Some(bytes) = string_bytes(&op.operands, 2) {
                show_string(&bytes, state, page, out);
            }
        }
        "TJ" => {
            if let Some(Object::Array(arr)) = op.operands.first() {
                for item in arr {
                    match item {
                        Object::String(bytes, _) => show_string(bytes, state, page, out),
                        Object::Integer(kern) => state.apply_kerning(*kern as f32),
                        Object::Real(kern) => state.apply_kerning(*kern as f32),
                        _ => {}
                    }
                }
            }
        }

        // ── XObject (Form) ─────────────────────────────────────────────────────
        "Do" => {
            if let Some(name_bytes) = op.operands.first().and_then(|o| o.as_name().ok()) {
                let name = String::from_utf8_lossy(name_bytes).into_owned();
                invoke_xobject(&name, state, resources, doc, page, out, out_rects);
            }
        }

        // ── Path Construction ──────────────────────────────────────────────────
        "m" | "l" => {
            if let (Some(x), Some(y)) = (f32_op(&op.operands, 0), f32_op(&op.operands, 1)) {
                curr_path.push(matrix_pt(state.ctm, x, y));
            }
        }
        "re" => {
            if let (Some(x), Some(y), Some(w), Some(h)) = (
                f32_op(&op.operands, 0),
                f32_op(&op.operands, 1),
                f32_op(&op.operands, 2),
                f32_op(&op.operands, 3),
            ) {
                // To properly map the bounds of a rectangle via CTM, project the min/max
                let (px0, py0) = matrix_pt(state.ctm, x, y);
                let (px1, py1) = matrix_pt(state.ctm, x + w, y + h);
                let min_x = px0.min(px1);
                let max_x = px0.max(px1);
                let min_y = py0.min(py1);
                let max_y = py0.max(py1);
                curr_rects.push(GraphicRect {
                    x: min_x,
                    y: min_y,
                    width: (max_x - min_x).abs(),
                    height: (max_y - min_y).abs(),
                    page,
                });
            }
        }
        "n" => {
            curr_path.clear();
            curr_rects.clear();
        }
        // ── Path Painting ──────────────────────────────────────────────────────
        "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
            // Emitting rectangles from drawn path lines
            if curr_path.len() >= 2 {
                for i in 0..(curr_path.len() - 1) {
                    let p0 = curr_path[i];
                    let p1 = curr_path[i + 1];
                    // If it is roughly a horizontal line
                    if (p0.1 - p1.1).abs() < 2.0 {
                        let min_x = p0.0.min(p1.0);
                        let w = (p0.0 - p1.0).abs();
                        if w > 1.0 {
                            out_rects.push(GraphicRect {
                                x: min_x,
                                y: p0.1,
                                width: w,
                                height: state.line_width,
                                page,
                            });
                        }
                    }
                }
            }
            out_rects.extend(curr_rects.drain(..));
            curr_path.clear();
        }

        _ => {}
    }
}

// ─── Text showing helper ──────────────────────────────────────────────────────

/// Decode `bytes` using the current font and emit one `RichSpan` per glyph.
fn show_string(bytes: &[u8], state: &mut GraphicsState, page: u32, out: &mut Vec<RichSpan>) {
    let font = match state.font.clone() {
        Some(f) => f,
        None => return,
    };

    let eff_size = state.effective_font_size().abs().max(0.5);
    let (fname, bold, italic) = (font.base_name.clone(), font.is_bold, font.is_italic);
    let fill = state.fill_color;

    for (ch, width_units) in font.decode(bytes) {
        if ch == '\0' {
            continue;
        }
        let (x, y) = state.glyph_origin();

        // Advance width in page coordinates.
        let page_width = {
            let th = state.horiz_scale / 100.0;
            (width_units / 1000.0 * state.font_size
                + state.char_spacing
                + if ch == ' ' { state.word_spacing } else { 0.0 })
                * th
                * (state.trm()[0].abs().max(0.1))
                / state.font_size.abs().max(0.1)
        };

        // Emit span (skip zero-width and pure-control chars).
        if !ch.is_control() || ch == ' ' {
            out.push(RichSpan {
                text: ch.to_string(),
                x,
                y,
                width: page_width.abs(),
                font_size: eff_size,
                font_name: fname.clone(),
                is_bold: bold,
                is_italic: italic,
                is_underlined: false,
                is_strikethrough: false,
                fill_color: fill,
                page,
            });
        }

        state.advance_glyph(width_units, ch == ' ');
    }
}

// ─── XObject invocation ──────────────────────────────────────────────────────

fn invoke_xobject(
    name: &str,
    state: &mut GraphicsState,
    resources: &Dictionary,
    doc: &Document,
    page: u32,
    out: &mut Vec<RichSpan>,
    out_rects: &mut Vec<GraphicRect>,
) {
    // Resolve XObject dict from resources.
    let xobj_dict = match resources.get(b"XObject").ok() {
        Some(Object::Dictionary(d)) => d,
        Some(Object::Reference(id)) => match doc.get_dictionary(*id).ok() {
            Some(d) => d,
            None => return,
        },
        _ => return,
    };

    let stream_id = match xobj_dict.get(name.as_bytes()).ok() {
        Some(Object::Reference(id)) => *id,
        _ => return,
    };

    let stream = match doc.get_object(stream_id).ok() {
        Some(Object::Stream(s)) => s,
        _ => return,
    };

    // Only process Form XObjects.
    let subtype = stream
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_default();
    if subtype != "Form" {
        return;
    }

    // Resolve XObject's local resources (fall back to parent).
    let local_res = stream
        .dict
        .get(b"Resources")
        .ok()
        .and_then(|o| match o {
            Object::Dictionary(d) => Some(d.clone()),
            Object::Reference(id) => doc.get_dictionary(*id).cloned().ok(),
            _ => None,
        })
        .unwrap_or_else(|| resources.clone());

    // Decode content.
    let raw = match stream.decompressed_content() {
        Ok(b) => b,
        Err(_) => return,
    };
    let content = match lopdf::content::Content::decode(&raw) {
        Ok(c) => c,
        Err(_) => return,
    };

    // XObject has its own graphics state scope; apply /Matrix if present.
    state.push();
    if let Some(m) = stream
        .dict
        .get(b"Matrix")
        .ok()
        .and_then(|o| parse_matrix_obj(o))
    {
        state.ctm = matrix_mul(state.ctm, m);
    }

    process_ops(
        &content.operations,
        state,
        &local_res,
        doc,
        page,
        out,
        out_rects,
    );
    state.pop();
}

// ─── Font resolution ─────────────────────────────────────────────────────────

fn resolve_font(name: &str, resources: &Dictionary, doc: &Document) -> Option<Font> {
    let font_res = resources.get(b"Font").ok()?;
    let font_dict_res = match font_res {
        Object::Dictionary(d) => d,
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        _ => return None,
    };

    let font_obj = font_dict_res.get(name.as_bytes()).ok()?;
    let font_dict = match font_obj {
        Object::Dictionary(d) => d,
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        _ => return None,
    };

    Some(Font::load(font_dict, doc))
}

// ─── Operand helpers ─────────────────────────────────────────────────────────

fn f32_op(ops: &[Object], idx: usize) -> Option<f32> {
    match ops.get(idx)? {
        Object::Real(f) => Some(*f as f32),
        Object::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

fn string_bytes(ops: &[Object], idx: usize) -> Option<Vec<u8>> {
    match ops.get(idx)? {
        Object::String(b, _) => Some(b.clone()),
        _ => None,
    }
}

fn parse_matrix(ops: &[Object]) -> Option<Matrix> {
    let nums: Vec<f32> = ops
        .iter()
        .filter_map(|o| match o {
            Object::Real(f) => Some(*f as f32),
            Object::Integer(i) => Some(*i as f32),
            _ => None,
        })
        .collect();
    if nums.len() >= 6 {
        Some([nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]])
    } else {
        None
    }
}

fn parse_matrix_obj(obj: &Object) -> Option<Matrix> {
    let arr = match obj {
        Object::Array(a) => a,
        _ => return None,
    };
    parse_matrix(arr)
}

fn cmyk_to_rgb(c: f32, m: f32, y: f32, k: f32) -> (f32, f32, f32) {
    let r = (1.0 - c) * (1.0 - k);
    let g = (1.0 - m) * (1.0 - k);
    let b = (1.0 - y) * (1.0 - k);
    (r, g, b)
}
