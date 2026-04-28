//! Typst compilation and PDF export.
//!
//! Delegates world construction to `marksmen_render::MarksmenWorld` (the SSOT).
//! This module is responsible only for the compile → PDF export step.
//!
//! ## Theorem: Compilation Determinism
//! Given identical Typst source and font set, the compiler always produces
//! byte-identical PDF output (modulo PDF creation timestamps, which are
//! set to a fixed value inside `MarksmenWorld::today`).

use anyhow::Result;
use marksmen_core::config::Config;
use marksmen_render::MarksmenWorld;
use typst::World;

/// Compile Typst source markup to PDF bytes.
pub fn compile_to_pdf(
    typst_source: &str,
    _config: &Config,
    base_path: Option<std::path::PathBuf>,
) -> Result<Vec<u8>> {
    let world = MarksmenWorld::new(typst_source, base_path)?;

    let document = typst::compile(&world).output.map_err(|diagnostics| {
        let messages: Vec<String> = diagnostics
            .iter()
            .map(|d| {
                let mut loc = String::new();
                if let Some(id) = d.span.id() {
                    if let Ok(src) = world.source(id) {
                        if let Some(range) = src.range(d.span) {
                            let start = range.start.saturating_sub(40);
                            let end = (range.end + 40).min(src.text().len());
                            loc = format!(" near `{:?}`", &src.text()[start..end]);
                        }
                    }
                }
                format!("{:?}{}: {}", d.severity, loc, d.message)
            })
            .collect();
        anyhow::anyhow!("Typst compilation failed:\n{}", messages.join("\n"))
    })?;

    let pdf_bytes =
        typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default()).map_err(|errs| {
            let msgs: Vec<String> = errs.iter().map(|e| format!("{:?}", e)).collect();
            anyhow::anyhow!("PDF export failed:\n{}", msgs.join("\n"))
        })?;

    Ok(pdf_bytes)
}
