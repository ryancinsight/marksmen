//! Markdown parsing layer.
//!
//! Wraps `pulldown-cmark` with `ENABLE_MATH` and GFM extensions enabled,
//! producing a typed event stream from Markdown input. The optional
//! `attribute_pass` sub-module applies a post-parse intercept that converts
//! freestanding `{.ClassName}` blocks into `AnnotatedEvent::Attributed` tags.

pub mod attribute_pass;
pub mod parser;

pub use attribute_pass::{AnnotatedEvent, intercept};
