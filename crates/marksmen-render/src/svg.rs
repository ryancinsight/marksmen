//! SVG byte slice → PNG rasterization via `usvg` + `resvg` + `tiny-skia`.
//!
//! ## Theorem: Rendered Pixel Correctness
//! The `usvg` tree faithfully represents any SVG produced by `marksmen-mermaid`'s
//! `render_graph_to_svg` because that function emits only:
//! - `<rect>`, `<polyline>`, `<polygon>`, `<text>`, `<svg>` elements
//! - Standard SVG attributes with no `<foreignObject>` or CSS animations
//!   All elements are within `usvg`'s supported subset, so no information is lost
//!   during parsing.

/// Rasterize a raw SVG byte slice to a PNG byte buffer.
///
/// # Returns
/// `Some((png_bytes, width_px, height_px))` on success, `None` on any failure.
pub fn svg_bytes_to_png(svg_data: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_data, &opt).ok()?;
    let size = tree.size();
    let width = size.width() as u32;
    let height = size.height() as u32;
    if width == 0 || height == 0 {
        return None;
    }
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    let png_data = pixmap.encode_png().ok()?;
    Some((png_data, width, height))
}
