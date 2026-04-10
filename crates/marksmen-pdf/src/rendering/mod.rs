//! Rendering layer: Typst markup → PDF bytes.
//!
//! Implements the Typst `World` trait to provide the compiler with
//! source files, fonts, and library access, then compiles and exports
//! the document to PDF.

pub mod compiler;
