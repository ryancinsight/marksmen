//! PDF graphics state for text-coordinate tracking.
//!
//! Implements §8.4 (graphics state) and §9.3 (text state) of ISO 32000-1.

use crate::reader::font::Font;

/// 3×3 affine matrix in PDF row-major order: [a b 0 / c d 0 / e f 1].
/// Stored as [a, b, c, d, e, f].
pub type Matrix = [f32; 6];

#[inline]
pub fn identity() -> Matrix {
    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

/// result = a × b.
pub fn matrix_mul(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

#[inline]
pub fn matrix_pt(m: Matrix, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// Complete graphics + text state for one content-stream nesting level.
#[derive(Debug, Clone)]
pub struct GraphicsState {
    pub ctm: Matrix,
    pub tm: Matrix,
    pub tlm: Matrix,
    pub char_spacing: f32,
    pub word_spacing: f32,
    pub horiz_scale: f32, // percent, default 100
    pub leading: f32,
    pub font_size: f32,
    pub render_mode: u8,
    pub line_width: f32,
    pub fill_color: (f32, f32, f32),
    pub stroke_color: (f32, f32, f32),
    pub font: Option<Font>,
    pub font_resource_name: String,
    stack: Vec<GsSnapshot>,
}

impl Default for GraphicsState {
    fn default() -> Self {
        GraphicsState {
            ctm: identity(),
            tm: identity(),
            tlm: identity(),
            char_spacing: 0.0,
            word_spacing: 0.0,
            horiz_scale: 100.0,
            leading: 0.0,
            font_size: 12.0,
            render_mode: 0,
            line_width: 1.0,
            fill_color: (0.0, 0.0, 0.0),
            stroke_color: (0.0, 0.0, 0.0),
            font: None,
            font_resource_name: String::new(),
            stack: Vec::new(),
        }
    }
}

impl GraphicsState {
    pub fn push(&mut self) {
        self.stack.push(GsSnapshot {
            ctm: self.ctm,
            tm: self.tm,
            tlm: self.tlm,
            char_spacing: self.char_spacing,
            word_spacing: self.word_spacing,
            horiz_scale: self.horiz_scale,
            leading: self.leading,
            font_size: self.font_size,
            render_mode: self.render_mode,
            line_width: self.line_width,
            fill_color: self.fill_color,
            stroke_color: self.stroke_color,
            font: self.font.clone(),
            font_resource_name: self.font_resource_name.clone(),
        });
    }

    pub fn pop(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.ctm = s.ctm;
            self.tm = s.tm;
            self.tlm = s.tlm;
            self.char_spacing = s.char_spacing;
            self.word_spacing = s.word_spacing;
            self.horiz_scale = s.horiz_scale;
            self.leading = s.leading;
            self.font_size = s.font_size;
            self.render_mode = s.render_mode;
            self.line_width = s.line_width;
            self.fill_color = s.fill_color;
            self.stroke_color = s.stroke_color;
            self.font = s.font;
            self.font_resource_name = s.font_resource_name;
        }
    }

    /// Text Rendering Matrix (TRM): maps glyph-space → page coordinates.
    ///  TRM = [Tfs·Th  0  0  Tfs  0  0] × TM × CTM
    pub fn trm(&self) -> Matrix {
        let th = self.horiz_scale / 100.0;
        let ts = [self.font_size * th, 0.0, 0.0, self.font_size, 0.0, 0.0];
        matrix_mul(matrix_mul(ts, self.tm), self.ctm)
    }

    /// Page-coordinate origin of the current glyph (TRM translation column).
    pub fn glyph_origin(&self) -> (f32, f32) {
        let t = self.trm();
        (t[4], t[5])
    }

    /// Effective font size in page space (y-scale magnitude of TRM).
    pub fn effective_font_size(&self) -> f32 {
        let t = self.trm();
        (t[1] * t[1] + t[3] * t[3]).sqrt()
    }

    /// Advance TM by one glyph of `width_units` (in 1/1000 text units).
    pub fn advance_glyph(&mut self, width_units: f32, is_space: bool) {
        let th = self.horiz_scale / 100.0;
        let tx = (width_units / 1000.0 * self.font_size
            + self.char_spacing
            + if is_space { self.word_spacing } else { 0.0 })
            * th;
        self.tm = matrix_mul([1.0, 0.0, 0.0, 1.0, tx, 0.0], self.tm);
    }

    /// Apply TJ kerning offset (in 1/1000 Tfs units).
    pub fn apply_kerning(&mut self, kern: f32) {
        let th = self.horiz_scale / 100.0;
        let tx = -(kern / 1000.0) * self.font_size * th;
        self.tm = matrix_mul([1.0, 0.0, 0.0, 1.0, tx, 0.0], self.tm);
    }
}

#[derive(Debug, Clone)]
struct GsSnapshot {
    ctm: Matrix,
    tm: Matrix,
    tlm: Matrix,
    char_spacing: f32,
    word_spacing: f32,
    horiz_scale: f32,
    leading: f32,
    font_size: f32,
    render_mode: u8,
    line_width: f32,
    fill_color: (f32, f32, f32),
    stroke_color: (f32, f32, f32),
    font: Option<Font>,
    font_resource_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_shifts_x_by_font_size() {
        let mut gs = GraphicsState::default();
        gs.font_size = 10.0;
        gs.advance_glyph(1000.0, false); // 1000/1000 * 10 = 10pt
        let (x, _) = gs.glyph_origin();
        assert!((x - 10.0).abs() < 0.01, "x={}", x);
    }

    #[test]
    fn kerning_reduces_x() {
        let mut gs = GraphicsState::default();
        gs.font_size = 10.0;
        gs.apply_kerning(500.0); // -5pt
        let (x, _) = gs.glyph_origin();
        assert!((x + 5.0).abs() < 0.01, "x={}", x);
    }
}
