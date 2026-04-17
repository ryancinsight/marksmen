//! `marksmen-render` — shared rasterization primitives for the marksmen workspace.
//!
//! Provides three conversion functions used by both `marksmen-docx` and `marksmen-pdf`:
//!
//! | Function | Input | Output |
//! |---|---|---|
//! | [`render_math_to_png`] | LaTeX math string | `(png_bytes, width, height)` |
//! | [`render_mmd_to_png`] | Mermaid source string | `(png_bytes, width, height)` |
//! | [`svg_bytes_to_png`] | SVG byte slice | `(png_bytes, width, height)` |
//!
//! ## Architecture invariants
//! - No circular imports: `marksmen-render` depends only on `marksmen-mermaid`
//!   (not on `marksmen-docx` or `marksmen-pdf`).
//! - `MarksmenWorld` (the Typst compiler shim) is defined here as the SSOT;
//!   `marksmen-pdf` re-exports it via `marksmen_render::world`.
//! - All functions return `Option` — callers are responsible for fallback behaviour
//!   appropriate to their output format.

pub mod math;
pub mod mermaid;
pub mod svg;
pub mod world;

pub use math::render_math_to_png;
pub use mermaid::render_mmd_to_png;
pub use svg::svg_bytes_to_png;
pub use world::MarksmenWorld;
