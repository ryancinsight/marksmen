//! # marksmen-core
//!
//! Core library for converting Markdown documents to PDF with native math
//! equation support. Uses `pulldown-cmark` for Markdown parsing (with
//! `ENABLE_MATH` for `$...$` and `$$...$$` delimiters), translates the
//! event stream to Typst markup, and compiles to PDF via `typst-pdf`.
//!
//! ## Pipeline
//!
//! ```text
//! Markdown + LaTeX Math
//!   → pulldown-cmark (Event Stream)
//!   → TypstTranslator (Typst Markup String)
//!   → typst::compile (PagedDocument)
//!   → typst_pdf::pdf (PDF Bytes)
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use marksmen_core::{convert, Config};
//!
//! let markdown = "# Hello\n\nInline math: $E = mc^2$\n\n$$\\int_0^1 x\\,dx = \\frac{1}{2}$$";
//! let config = Config::default();
//! let pdf_bytes = convert(markdown, &config).expect("conversion failed");
//! std::fs::write("output.pdf", &pdf_bytes).unwrap();
//! ```

pub mod config;
pub mod parsing;

pub use config::Config;
pub use parsing::parser::parse;
