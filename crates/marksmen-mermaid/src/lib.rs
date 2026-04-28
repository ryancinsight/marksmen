//! Pure-Rust Mermaid to Typst rendering engine.
//!
//! Parses Mermaid flowchart syntax, mathematically computes a Sugiyama DAG layout,
//! and emits strict absolute Typst placement coordinates (`#place`).

pub mod graph;
pub mod layout;
pub mod parsing;
pub mod rendering;
