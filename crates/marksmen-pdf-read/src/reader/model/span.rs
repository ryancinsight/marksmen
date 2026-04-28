//! Rich text span model — the canonical intermediate representation for PDF extraction.
//!
//! `RichSpan` carries all typographic and spatial properties needed to reconstruct
//! document structure and drive any downstream writer (Markdown, DOCX, etc.).

/// A single decoded text span with full spatial and typographic metadata.
///
/// Positions are in PDF page coordinates: origin at bottom-left, Y increases upward,
/// units are points (1 pt = 1/72 inch).
#[derive(Debug, Clone, PartialEq)]
pub struct RichSpan {
    /// Decoded Unicode text for this span.
    pub text: String,
    /// Left edge of the span in page coordinates (pts).
    pub x: f32,
    /// Bottom edge of the span (baseline) in page coordinates (pts).
    pub y: f32,
    /// Advance width of the span in page coordinates (pts).
    pub width: f32,
    /// Rendered font size (pts) — after CTM scaling.
    pub font_size: f32,
    /// PDF font resource name (e.g. "Arial-BoldMT", "TimesNewRomanPS-BoldItalicMT").
    pub font_name: String,
    /// True when the font name contains "Bold" or the font descriptor flags bit 18 is set.
    pub is_bold: bool,
    /// True when the font name contains "Italic" / "Oblique" or descriptor flags bit 7 is set.
    pub is_italic: bool,
    /// True when a graphical vector rectangle intersects this span's horizontal and baseline.
    pub is_underlined: bool,
    /// True when a graphical vector rectangle intersects this span's horizontal and mid-bounds.
    pub is_strikethrough: bool,
    /// RGB fill color in [0,1] range. Default (0,0,0) = black.
    pub fill_color: (f32, f32, f32),
    /// 1-indexed page number.
    pub page: u32,
}

impl RichSpan {
    /// Approximate right edge: x + width.
    #[inline]
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// Approximate top edge: y + font_size.
    #[inline]
    pub fn top(&self) -> f32 {
        self.y + self.font_size
    }

    /// True if this span and `other` share the same visual line (Y-baseline within tolerance).
    pub fn same_line_as(&self, other: &RichSpan, tolerance: f32) -> bool {
        (self.y - other.y).abs() <= tolerance
    }
}
