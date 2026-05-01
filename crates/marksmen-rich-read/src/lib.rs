//! marksmen-rich-read — RTF to Markdown conversion.

pub mod codepage;
mod parser;

pub use parser::parse_rtf;

#[cfg(test)]
mod tests;
