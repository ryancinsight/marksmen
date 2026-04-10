//! Core logic for `marksmen-odt`.
//!
//! Provides the mathematical translation of the generic Markdown AST into OpenDocument Text (`.odt`)
//! XML domains, fundamentally bypassing the Typst rendering component to yield native editable
//! ODT formats encoded as zipped `.xml` archives.

pub mod translation;
pub mod rendering;

use marksmen_core::config::Config;
use pulldown_cmark::Event;
use anyhow::Result;
use std::path::Path;

/// Translates the verified Markdown AST and assembles a fully sealed OpenDocument (`.odt`)
/// byte payload structured as a compliant ZIP archive in memory.
pub fn translate_and_render<'a>(events: &[Event<'a>], config: &Config, input_dir: &Path) -> Result<Vec<u8>> {
    // Stage 1: Mathematical DOM Translation (AST -> XML elements)
    let odt_dom = translation::translate(events, config, input_dir)?;

    // Stage 2: Archive Rendering (DOM -> Encrypted/Zipped .odt standard)
    let odt_bytes = rendering::assemble_archive(odt_dom)?;

    Ok(odt_bytes)
}
