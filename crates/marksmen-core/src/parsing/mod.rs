//! Markdown parsing layer.
//!
//! Wraps `pulldown-cmark` with `ENABLE_MATH` and GFM extensions enabled,
//! producing a typed event stream from Markdown input.

pub mod parser;
