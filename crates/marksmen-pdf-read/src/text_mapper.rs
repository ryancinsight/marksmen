//! Glyph-level text position extractor for PDF annotation localization.
//!
//! Walks page content streams via `lopdf`, tracks text matrices (`Tm`),
//! and accumulates character positions in page coordinates so that annotation
//! `Rect`/`QuadPoints` can be mapped back to text ranges.

use anyhow::Result;
use lopdf::{Document, Object, ObjectId};

/// A single run of text with its bounding rectangle in page coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub rect: super::Rect,
    pub font_size: f32,
    pub font_name: String,
    pub is_bold: bool,
}

/// Extract all text runs with page-coordinate bounding boxes from a PDF page.
///
/// This walks the page content stream(s), tracking graphics state transforms,
/// and estimates a bounding box for each text run using the text matrix and
/// font metrics. It is intentionally approximate: full glyph metric extraction
/// would require parsing font program data (CFF/TTF), which is out of scope.
/// For annotation localization, approximate boxes are sufficient.
pub fn extract_text_runs(document: &Document, page_id: ObjectId) -> Result<Vec<TextRun>> {
    let page_dict = document.get_dictionary(page_id)?;

    // Resolve content stream(s).
    // Contents can be a direct stream reference, a direct array, or a reference resolving to an array.
    let contents: Vec<ObjectId> = {
        let raw = match page_dict.get(b"Contents") {
            Ok(o) => o.clone(),
            Err(_) => return Ok(Vec::new()),
        };
        match raw {
            Object::Array(arr) => arr.iter().filter_map(|o| o.as_reference().ok()).collect(),
            Object::Reference(id) => {
                match document.get_object(id)? {
                    // ref → array: collect the inner refs
                    Object::Array(arr) => {
                        arr.iter().filter_map(|o| o.as_reference().ok()).collect()
                    }
                    // ref → stream: use the ref itself
                    _ => vec![id],
                }
            }
            _ => return Ok(Vec::new()),
        }
    };
    tracing::debug!("Resolved {} content stream IDs for page", contents.len());

    let mut font_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Ok(resources_obj) = page_dict.get(b"Resources") {
        let resources_dict = match resources_obj {
            Object::Dictionary(d) => Some(d),
            Object::Reference(id) => document.get_dictionary(*id).ok(),
            _ => None,
        };
        if let Some(res) = resources_dict
            && let Ok(fonts_obj) = res.get(b"Font") {
                let fonts_dict = match fonts_obj {
                    Object::Dictionary(d) => Some(d),
                    Object::Reference(id) => document.get_dictionary(*id).ok(),
                    _ => None,
                };
                if let Some(fd) = fonts_dict {
                    for (k, v) in fd.iter() {
                        let font_obj_res = match v {
                            Object::Dictionary(d) => Ok(d),
                            Object::Reference(id) => document.get_dictionary(*id),
                            _ => Err(lopdf::Error::DictKey),
                        };
                        if let Ok(font_obj) = font_obj_res
                            && let Ok(base_font) =
                                font_obj.get(b"BaseFont").and_then(|o| o.as_name())
                            {
                                font_map.insert(
                                    String::from_utf8_lossy(k).into_owned(),
                                    String::from_utf8_lossy(base_font).into_owned(),
                                );
                            }
                    }
                }
            }
    }

    let mut runs: Vec<TextRun> = Vec::new();
    let mut state = GraphicsState::default();

    let res_dict = page_dict
        .get(b"Resources")
        .and_then(|o| match o {
            Object::Dictionary(d) => Ok(d.clone()),
            Object::Reference(id) => document.get_dictionary(*id).cloned(),
            _ => Err(lopdf::Error::DictKey),
        })
        .unwrap_or_else(|_| lopdf::Dictionary::new());

    for content_id in contents {
        let stream = match document.get_object(content_id)? {
            Object::Stream(s) => s,
            _ => continue,
        };
        let content = match stream.decompressed_content() {
            Ok(raw_bytes) => {
                tracing::debug!(
                    "Stream raw bytes (first {}): {:?}",
                    raw_bytes.len().min(80),
                    &raw_bytes[..raw_bytes.len().min(80)]
                );
                match lopdf::content::Content::decode(&raw_bytes) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::debug!("Content decode after decompression failed: {}", e);
                        lopdf::content::Content { operations: vec![] }
                    }
                }
            }
            Err(e) => {
                tracing::debug!("Decompression failed: {}", e);
                lopdf::content::Content { operations: vec![] }
            }
        };

        tracing::debug!(
            "Page content stream has {} operations. First op: '{}'",
            content.operations.len(),
            content
                .operations
                .first()
                .map(|o| o.operator.as_ref())
                .unwrap_or("None")
        );

        let mut layout_bounds = Vec::new();
        process_operations(
            document,
            &res_dict,
            &content.operations,
            &mut state,
            &font_map,
            &mut runs,
            &mut layout_bounds,
        );
    }

    Ok(runs)
}

fn process_operations(
    document: &Document,
    resources: &lopdf::Dictionary,
    operations: &[lopdf::content::Operation],
    state: &mut GraphicsState,
    font_map: &std::collections::HashMap<String, String>,
    runs: &mut Vec<TextRun>,
    layout_bounds: &mut Vec<super::Rect>,
) {
    for operation in operations {
        match operation.operator.as_ref() {
            "q" => state.push_gs(),
            "Q" => state.pop_gs(),
            "cm" => {
                if let Some(m) = parse_matrix(&operation.operands) {
                    state.ctm = matrix_concat(state.ctm, m);
                }
            }
            "re" => {
                if let (Some(x), Some(y), Some(w), Some(h)) = (
                    operand_f32(&operation.operands, 0),
                    operand_f32(&operation.operands, 1),
                    operand_f32(&operation.operands, 2),
                    operand_f32(&operation.operands, 3),
                ) {
                    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
                    let mut xs = Vec::with_capacity(4);
                    let mut ys = Vec::with_capacity(4);
                    for (cx, cy) in &corners {
                        let (px, py) = matrix_point(state.ctm, *cx, *cy);
                        xs.push(px);
                        ys.push(py);
                    }
                    layout_bounds.push(super::Rect {
                        llx: xs.iter().cloned().fold(f32::INFINITY, f32::min),
                        lly: ys.iter().cloned().fold(f32::INFINITY, f32::min),
                        urx: xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
                        ury: ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
                    });
                }
            }
            "Do" => {
                // XObject Traversal
                if let Some(name_obj) = operation.operands.first()
                    && let Ok(name) = name_obj.as_name() {
                        println!(
                            "DEBUG: Found Do operator for XObject: {}",
                            String::from_utf8_lossy(name)
                        );
                        if let Ok(xobjs) = resources.get(b"XObject") {
                            let xdict = match xobjs {
                                Object::Dictionary(d) => Some(d),
                                Object::Reference(id) => document.get_dictionary(*id).ok(),
                                _ => None,
                            };
                            if let Some(xd) = xdict {
                                if let Ok(xobj_ref) = xd.get(name) {
                                    if let Ok(Object::Stream(xstream)) = match xobj_ref {
                                        Object::Reference(id) => document.get_object(*id),
                                        obj => Ok(obj),
                                    } {
                                        if let Ok(xcontent) = xstream.decode_content() {
                                            tracing::debug!(
                                                "Extracted {} operations from XObject",
                                                xcontent.operations.len()
                                            );
                                            // Extract potential local resources for this XObject
                                            let local_res = xstream
                                                .dict
                                                .get(b"Resources")
                                                .and_then(|o| match o {
                                                    Object::Dictionary(d) => Ok(d.clone()),
                                                    Object::Reference(id) => {
                                                        document.get_dictionary(*id).cloned()
                                                    }
                                                    _ => Err(lopdf::Error::DictKey),
                                                })
                                                .unwrap_or_else(|_| resources.clone());

                                            state.push_gs(); // Isolate XObject state
                                            process_operations(
                                                document,
                                                &local_res,
                                                &xcontent.operations,
                                                state,
                                                font_map,
                                                runs,
                                                layout_bounds,
                                            );
                                            state.pop_gs();
                                        } else {
                                            tracing::debug!("Failed to decode XObject stream");
                                        }
                                    } else {
                                        tracing::debug!("XObject is not a stream");
                                    }
                                } else {
                                    tracing::debug!("XObject name not found in XObject dict");
                                }
                            }
                        } else {
                            tracing::debug!("No XObject dictionary found in resources");
                        }
                    }
            }
            "Tm" => {
                // Set text matrix (and text line matrix).
                if let Some(m) = parse_matrix(&operation.operands) {
                    state.tm = m;
                    state.tlm = m;
                }
            }
            "Td" => {
                // Move text position.
                if let (Some(tx), Some(ty)) = (
                    operand_f32(&operation.operands, 0),
                    operand_f32(&operation.operands, 1),
                ) {
                    state.tlm[4] += tx;
                    state.tlm[5] += ty;
                    state.tm = state.tlm;
                }
            }
            "TD" => {
                // Move text position and set leading.
                if let (Some(tx), Some(ty)) = (
                    operand_f32(&operation.operands, 0),
                    operand_f32(&operation.operands, 1),
                ) {
                    state.tlm[4] += tx;
                    state.tlm[5] += ty;
                    state.tm = state.tlm;
                    state.leading = -ty;
                }
            }
            "T*" => {
                // Move to start of next line.
                state.tlm[5] -= state.leading;
                state.tm = state.tlm;
            }
            "Tf" => {
                // Set font and size.
                if let Some(font_name_obj) = operation.operands.first()
                    && let Ok(name) = font_name_obj.as_name() {
                        let key = String::from_utf8_lossy(name).into_owned();
                        if let Some(base_font) = font_map.get(&key) {
                            state.font_name = base_font.clone();
                        } else {
                            state.font_name = key;
                        }
                    }
                if let Some(size) = operand_f32(&operation.operands, 1) {
                    state.font_size = size;
                }
            }
            "TJ" => {
                // Show text with individual glyph positioning.
                if let Some(arr) = operation.operands.first().and_then(|o| o.as_array().ok()) {
                    for item in arr {
                        match item {
                            Object::String(s, _) => {
                                let text = String::from_utf8_lossy(s).into_owned();
                                if !text.is_empty() {
                                    if let Some(rect) = state.text_rect(&text, layout_bounds) {
                                        let is_bold =
                                            state.font_name.to_lowercase().contains("bold");
                                        runs.push(TextRun {
                                            text: text.clone(),
                                            rect,
                                            font_size: state.font_size,
                                            font_name: state.font_name.clone(),
                                            is_bold,
                                        });
                                    }
                                    state.advance_tm(&text, layout_bounds);
                                }
                            }
                            Object::Integer(offset) => {
                                // Negative offset means move right (glyph spacing in 1/1000 text units).
                                let tx = -(*offset as f32) * state.font_size / 1000.0;
                                state.tm[4] += tx;
                            }
                            Object::Real(offset) => {
                                let tx = -*offset * state.font_size / 1000.0;
                                state.tm[4] += tx;
                            }
                            _ => {}
                        }
                    }
                }
            }
            "Tj" | "'" | "\"" => {
                // Show text (simple string or with line spacing).
                if let Some(text) = operation
                    .operands
                    .first()
                    .and_then(|o| o.as_string().ok().map(|s| s.into_owned()))
                    && !text.is_empty() {
                        if let Some(rect) = state.text_rect(&text, layout_bounds) {
                            let is_bold = state.font_name.to_lowercase().contains("bold");
                            runs.push(TextRun {
                                text: text.clone(),
                                rect,
                                font_size: state.font_size,
                                font_name: state.font_name.clone(),
                                is_bold,
                            });
                        }
                        state.advance_tm(&text, layout_bounds);
                    }
            }
            _ => {}
        }
    }
}

/// 3×3 PDF graphics state matrix in row-major order:
/// [a b 0]
/// [c d 0]
/// [e f 1]
/// Stored as [a, b, c, d, e, f].
type Matrix = [f32; 6];

fn identity() -> Matrix {
    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

/// Multiply two matrices: result = a × b.
fn matrix_concat(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

/// Apply a matrix to a point (x, y).
fn matrix_point(m: Matrix, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn parse_matrix(ops: &[Object]) -> Option<Matrix> {
    if ops.len() >= 6 {
        let nums: Vec<f32> = ops.iter().filter_map(operand_f32_obj).collect();
        if nums.len() >= 6 {
            return Some([nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]]);
        }
    }
    None
}

fn operand_f32_obj(o: &Object) -> Option<f32> {
    match o {
        Object::Real(f) => Some(*f),
        Object::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

fn operand_f32(ops: &[Object], idx: usize) -> Option<f32> {
    ops.get(idx).and_then(operand_f32_obj)
}

/// Current graphics state relevant to text positioning.
#[derive(Debug, Clone)]
struct GraphicsState {
    ctm: Matrix, // Current transformation matrix
    tm: Matrix,  // Text matrix
    tlm: Matrix, // Text line matrix
    leading: f32,
    font_size: f32,
    font_name: String,
    gs_stack: Vec<GsSnapshot>,
}

#[derive(Debug, Clone)]
struct GsSnapshot {
    ctm: Matrix,
    tm: Matrix,
    tlm: Matrix,
    leading: f32,
    font_size: f32,
    font_name: String,
}

impl Default for GraphicsState {
    fn default() -> Self {
        GraphicsState {
            ctm: identity(),
            tm: identity(),
            tlm: identity(),
            leading: 0.0,
            font_size: 12.0,
            font_name: "Unknown".to_string(),
            gs_stack: Vec::new(),
        }
    }
}

impl GraphicsState {
    fn push_gs(&mut self) {
        self.gs_stack.push(GsSnapshot {
            ctm: self.ctm,
            tm: self.tm,
            tlm: self.tlm,
            leading: self.leading,
            font_size: self.font_size,
            font_name: self.font_name.clone(),
        });
    }

    fn pop_gs(&mut self) {
        if let Some(snap) = self.gs_stack.pop() {
            self.ctm = snap.ctm;
            self.tm = snap.tm;
            self.tlm = snap.tlm;
            self.leading = snap.leading;
            self.font_size = snap.font_size;
            self.font_name = snap.font_name;
        }
    }

    /// Compute the page-coordinate bounding rectangle for `text` at the current TM.
    /// Uses bounding box intersection mapping to replace the default length heuristics.
    fn text_rect(&self, text: &str, layout_bounds: &[super::Rect]) -> Option<super::Rect> {
        let narrow_chars = text
            .chars()
            .filter(|c| matches!(c, 'i' | 'l' | '1' | 't' | 'I' | '.' | ',' | ':' | ';' | ' '))
            .count();
        let normal_chars = text.len().saturating_sub(narrow_chars);
        let mut width = (normal_chars as f32 * 0.55 + narrow_chars as f32 * 0.25) * self.font_size;
        let height = self.font_size;

        let m = matrix_concat(self.ctm, self.tm);
        let (start_px, start_py) = matrix_point(m, 0.0, 0.0);

        // Exact Layout Bounds Intersection Overrides heuristic
        for bound in layout_bounds {
            // Check if text starts inside the exact cell layout bounding box (structural container)
            if start_px >= bound.llx
                && start_px <= bound.urx
                && start_py >= bound.lly
                && start_py <= bound.ury
            {
                let bound_width = bound.urx - bound.llx;
                // Verify bounds are plausible (e.g. not the full page boundary box itself)
                if bound_width > 0.0 && bound_width < 1000.0 {
                    let internal_x_scale = m[0].abs().max(0.001);
                    width = bound_width / internal_x_scale;
                }
                break;
            }
        }

        // Compute the four corners of the text box in text space.
        let corners = [(0.0, 0.0), (width, 0.0), (width, height), (0.0, height)];

        let mut xs = Vec::with_capacity(4);
        let mut ys = Vec::with_capacity(4);
        for (x, y) in &corners {
            let (px, py) = matrix_point(m, *x, *y);
            xs.push(px);
            ys.push(py);
        }
        Some(super::Rect {
            llx: xs.iter().cloned().fold(f32::INFINITY, f32::min),
            lly: ys.iter().cloned().fold(f32::INFINITY, f32::min),
            urx: xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
            ury: ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
        })
    }

    /// Advance the text matrix by the width of `text`.
    fn advance_tm(&mut self, text: &str, layout_bounds: &[super::Rect]) {
        let narrow_chars = text
            .chars()
            .filter(|c| matches!(c, 'i' | 'l' | '1' | 't' | 'I' | '.' | ',' | ':' | ';' | ' '))
            .count();
        let normal_chars = text.len().saturating_sub(narrow_chars);
        let mut width = (normal_chars as f32 * 0.55 + narrow_chars as f32 * 0.25) * self.font_size;

        let m = matrix_concat(self.ctm, self.tm);
        let (start_px, start_py) = matrix_point(m, 0.0, 0.0);
        for bound in layout_bounds {
            if start_px >= bound.llx
                && start_px <= bound.urx
                && start_py >= bound.lly
                && start_py <= bound.ury
            {
                let bound_width = bound.urx - bound.llx;
                if bound_width > 0.0 && bound_width < 1000.0 {
                    let internal_x_scale = m[0].abs().max(0.001);
                    width = bound_width / internal_x_scale;
                }
                break;
            }
        }

        self.tm[4] += width;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_concat_identity() {
        let i = identity();
        assert_eq!(matrix_concat(i, i), i);
    }

    #[test]
    fn matrix_concat_translation() {
        let t = [1.0, 0.0, 0.0, 1.0, 10.0, 20.0];
        let i = identity();
        let r = matrix_concat(i, t);
        assert_eq!(r, t);
    }

    #[test]
    fn matrix_point_basic() {
        let m = [1.0, 0.0, 0.0, 1.0, 5.0, 10.0];
        let (x, y) = matrix_point(m, 2.0, 3.0);
        assert!((x - 7.0).abs() < 1e-6);
        assert!((y - 13.0).abs() < 1e-6);
    }

    #[test]
    fn graphics_state_text_rect_basic() {
        let mut gs = GraphicsState::default();
        gs.font_size = 10.0;
        let rect = gs.text_rect("hello", &[]).unwrap();
        // width ≈ 5 * 10 * 0.5 = 25
        assert!(rect.urx - rect.llx > 20.0 && rect.urx - rect.llx < 30.0);
        assert!(rect.ury - rect.lly > 5.0 && rect.ury - rect.lly < 15.0);
    }

    #[test]
    fn rect_intersects_basic() {
        let a = super::super::Rect {
            llx: 0.0,
            lly: 0.0,
            urx: 10.0,
            ury: 10.0,
        };
        let b = super::super::Rect {
            llx: 5.0,
            lly: 5.0,
            urx: 15.0,
            ury: 15.0,
        };
        assert!(a.intersects(&b));
    }
}
