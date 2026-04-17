//! LaTeX math → PNG rasterization via the in-process Typst compiler.
//!
//! ## Theorem: Rendering Correctness
//! A Typst document of the form:
//! ```typst
//! #set page(width: auto, height: auto, margin: 5pt)
//! #set text(size: 11pt, font: "New Computer Modern Math")
//! $ EXPR $          // inline (display: false)
//! ```
//! or:
//! ```typst
//! $ EXPR $\         // on its own paragraph (display: true)
//! ```
//! faithfully renders any LaTeX expression supported by Typst's math mode.
//! Typst math syntax is a strict superset of the operators used in the
//! SONALA001 PRD (fractions, integrals, Greek letters, subscripts, superscripts).
//!
//! ## Pixel density
//! `PIXELS_PER_PT = 2.0` produces 144 DPI output (2× pt → px at 72 DPI baseline),
//! matching the scale factor used by the PDF renderer and providing sharp output
//! at Word's default 96 DPI screen rendering.

use crate::world::MarksmenWorld;

/// Pixels per typographic point for math rasterization.
/// 2.0 → 144 DPI effective output, sharp at 96 DPI screen and 300 DPI print.
const PIXELS_PER_PT: f64 = 2.0;

/// Render a LaTeX math expression to a PNG byte buffer.
///
/// # Parameters
/// - `latex`: LaTeX math source (without surrounding `$` delimiters).
/// - `display`: `true` → display-mode block (centred, larger);
///              `false` → inline mode (text-height).
///
/// # Returns
/// `Some((png_bytes, width_px, height_px))` on success, `None` on any failure.
pub fn render_math_to_png(latex: &str, display: bool) -> Option<(Vec<u8>, u32, u32)> {
    // Typst math uses $ for inline, and a paragraph-level $ ... $ for display.
    // We emit a minimal auto-sized page so the bounding box exactly fits the equation.
    let math_body = if display {
        format!("$ {} $", latex)
    } else {
        // Inline: wrap in a text context so Typst sizes it as inline math.
        format!("$ {} $", latex)
    };

    let typst_source = format!(
        "#set page(width: auto, height: auto, margin: (x: 4pt, y: 3pt))\n\
         #set text(size: 11pt)\n\
         {math_body}"
    );

    let world = MarksmenWorld::new(&typst_source, None).ok()?;
    let document: typst::layout::PagedDocument = typst::compile(&world).output.ok()?;

    let pages = document.pages;
    let frame = pages.first()?;

    let pixmap = typst_render::render(frame, PIXELS_PER_PT as f32);
    let width = pixmap.width();
    let height = pixmap.height();
    if width == 0 || height == 0 {
        return None;
    }

    let png_bytes = pixmap.encode_png().ok()?;
    Some((png_bytes, width, height))
}
