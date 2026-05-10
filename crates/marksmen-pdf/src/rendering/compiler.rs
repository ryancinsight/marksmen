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

    let mut options = typst_pdf::PdfOptions::default();
    if let Some(std_str) = &_config.pdf_standard {
        let standard = match std_str.to_lowercase().as_str() {
            "pdf-a" | "a-1b" | "pdf/a-1b" => Some(typst_pdf::PdfStandard::A_1b),
            "a-2b" | "pdf/a-2b" => Some(typst_pdf::PdfStandard::A_2b),
            "a-3b" | "pdf/a-3b" => Some(typst_pdf::PdfStandard::A_3b),
            _ => None,
        };
        if let Some(s) = standard {
            if let Ok(standards) = typst_pdf::PdfStandards::new(&[s]) {
                options.standards = standards;
            }
        }
    }

    let pdf_bytes = typst_pdf::pdf(&document, &options).map_err(|errs| {
        let msgs: Vec<String> = errs.iter().map(|e| format!("{:?}", e)).collect();
        anyhow::anyhow!("PDF export failed:\n{}", msgs.join("\n"))
    })?;

    Ok(pdf_bytes)
}
