//! Translation layer: pulldown-cmark events → Typst markup string.
//!
//! Dispatches each markdown event to the appropriate Typst markup generator.

pub mod elements;
pub mod math;
pub mod translator;
