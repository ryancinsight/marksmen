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
    let contents = match page_dict.get(b"Contents") {
        Ok(Object::Reference(id)) => {
            vec![*id]
        }
        Ok(Object::Array(arr)) => {
            arr.iter().filter_map(|o| o.as_reference().ok()).collect()
        }
        Ok(_) => return Ok(Vec::new()),
        Err(_) => return Ok(Vec::new()),
    };

    let mut runs: Vec<TextRun> = Vec::new();
    let mut state = GraphicsState::default();

    for content_id in contents {
        let stream = match document.get_object(content_id)? {
            Object::Stream(s) => s,
            _ => continue,
        };
        let content = stream.decode_content().unwrap_or_else(|_| lopdf::content::Content { operations: vec![] });

        for operation in &content.operations {
            match operation.operator.as_ref() {
                "q" => state.push_gs(),
                "Q" => state.pop_gs(),
                "cm" => {
                    // Concatenate matrix to CTM.
                    if let Some(m) = parse_matrix(&operation.operands) {
                        state.ctm = matrix_concat(state.ctm, m);
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
                    if let Some(size) = operand_f32(&operation.operands, 1) {
                        state.font_size = size;
                    }
                }
                "TJ" => {
                    // Show text with individual glyph positioning.
                    if let Some(arr) = operation.operands.first().and_then(|o| o.as_array().ok()) {
                        let mut text = String::new();
                        for item in arr {
                            match item {
                                Object::String(s, _) => {
                                    if let Ok(s) = std::str::from_utf8(s) {
                                        text.push_str(&String::from_utf8_lossy(s.as_bytes()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        if !text.is_empty() {
                            if let Some(rect) = state.text_rect(&text) {
                                runs.push(TextRun { text: text.clone(), rect });
                            }
                            state.advance_tm(&text);
                        }
                    }
                }
                "Tj" | "'" | "\"" => {
                    // Show text (simple string or with line spacing).
                    if let Some(text) = operation.operands.first().and_then(|o| {
                        o.as_string().ok().map(|s| s.into_owned())
                    }) {
                        if !text.is_empty() {
                            if let Some(rect) = state.text_rect(&text) {
                                runs.push(TextRun { text: text.clone(), rect });
                            }
                            state.advance_tm(&text);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(runs)
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
        Object::Real(f) => Some(*f as f32),
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
    ctm: Matrix,      // Current transformation matrix
    tm: Matrix,       // Text matrix
    tlm: Matrix,      // Text line matrix
    leading: f32,
    font_size: f32,
    gs_stack: Vec<GsSnapshot>,
}

#[derive(Debug, Clone)]
struct GsSnapshot {
    ctm: Matrix,
    tm: Matrix,
    tlm: Matrix,
    leading: f32,
    font_size: f32,
}

impl Default for GraphicsState {
    fn default() -> Self {
        GraphicsState {
            ctm: identity(),
            tm: identity(),
            tlm: identity(),
            leading: 0.0,
            font_size: 12.0,
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
        });
    }

    fn pop_gs(&mut self) {
        if let Some(snap) = self.gs_stack.pop() {
            self.ctm = snap.ctm;
            self.tm = snap.tm;
            self.tlm = snap.tlm;
            self.leading = snap.leading;
            self.font_size = snap.font_size;
        }
    }

    /// Compute the page-coordinate bounding rectangle for `text` at the current TM.
    ///
    /// Approximation: uses font_size as height and `text.len() * font_size * 0.5` as width.
    /// This is crude but sufficient for intersection tests with annotation rectangles.
    fn text_rect(&self, text: &str) -> Option<super::Rect> {
        let width = text.len() as f32 * self.font_size * 0.5;
        let height = self.font_size;
        // Compute the four corners of the text box in text space.
        let corners = [
            (0.0, 0.0),
            (width, 0.0),
            (width, height),
            (0.0, height),
        ];
        // Transform to user space (TM), then to page space (CTM).
        let m = matrix_concat(self.ctm, self.tm);
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
    fn advance_tm(&mut self, text: &str) {
        let width = text.len() as f32 * self.font_size * 0.5;
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
        let rect = gs.text_rect("hello").unwrap();
        // width ≈ 5 * 10 * 0.5 = 25
        assert!(rect.urx - rect.llx > 20.0 && rect.urx - rect.llx < 30.0);
        assert!(rect.ury - rect.lly > 5.0 && rect.ury - rect.lly < 15.0);
    }

    #[test]
    fn rect_intersects_basic() {
        let a = super::super::Rect { llx: 0.0, lly: 0.0, urx: 10.0, ury: 10.0 };
        let b = super::super::Rect { llx: 5.0, lly: 5.0, urx: 15.0, ury: 15.0 };
        assert!(a.intersects(&b));
    }
}
