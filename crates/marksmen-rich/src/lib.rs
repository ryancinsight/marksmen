//! marksmen-rich — Markdown to RTF conversion.

mod writer;

pub use writer::{convert, rtf_escape};

#[cfg(test)]
mod tests;
