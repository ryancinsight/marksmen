//! CLI runner: orchestrates file reading, conversion, and output writing.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use marksmen_core::Config;

use super::Args;

/// Run the conversion process based on CLI arguments.
pub fn run(args: Args) -> Result<()> {
    let mut config = Config::default();

    // Apply CLI overrides.
    if args.no_math {
        config.math.enabled = false;
    }
    if let Some(ref width) = args.page_width {
        config.page.width = width.clone();
    }
    if let Some(ref height) = args.page_height {
        config.page.height = height.clone();
    }
    if let Some(ref margin) = args.margin {
        config.page.margin_top = margin.clone();
        config.page.margin_right = margin.clone();
        config.page.margin_bottom = margin.clone();
        config.page.margin_left = margin.clone();
    }

    for input_path in &args.files {
        convert_file(input_path, &args, &config)?;
    }

    if args.watch {
        tracing::info!("Watch mode not yet implemented — converting once.");
        // TODO: Implement watch mode using `notify` crate.
    }

    Ok(())
}

/// Convert a single markdown file to PDF.
fn convert_file(input_path: &Path, args: &Args, config: &Config) -> Result<()> {
    tracing::info!(path = %input_path.display(), "Converting");

    let markdown = fs::read_to_string(input_path)
        .with_context(|| format!("Failed to read input file: {}", input_path.display()))?;

    if args.as_typst {
        // Output the intermediate Typst source for debugging.
        let (body, fm_config) = marksmen_core::config::frontmatter::parse_frontmatter(&markdown)?;
        let merged = config.merge_frontmatter(&fm_config);
        let events = marksmen_core::parsing::parser::parse(body);
        let typst_source =
            marksmen_pdf::translation::translator::translate(events, &merged)?;

        let output_path = args
            .output
            .clone()
            .unwrap_or_else(|| input_path.with_extension("typ"));

        fs::write(&output_path, &typst_source)
            .with_context(|| format!("Failed to write Typst output: {}", output_path.display()))?;

        tracing::info!(path = %output_path.display(), "Typst source written");
    } else if let Some(out_path) = &args.output {
        if out_path.extension().and_then(|e| e.to_str()) == Some("docx") {
            // DOCX conversion pipeline
            let (body, fm_config) = marksmen_core::config::frontmatter::parse_frontmatter(&markdown)?;
            let merged = config.merge_frontmatter(&fm_config);
            let events = marksmen_core::parsing::parser::parse(body);
            let input_dir = input_path.parent().unwrap_or_else(|| std::path::Path::new("."));
            let docx_bytes = marksmen_docx::translation::document::convert(events, &merged, input_dir)?;
            
            fs::write(&out_path, &docx_bytes)
                .with_context(|| format!("Failed to write DOCX output: {}", out_path.display()))?;
            
            tracing::info!(
                path = %out_path.display(),
                size_bytes = docx_bytes.len(),
                "DOCX written"
            );
            return Ok(());
        } else if out_path.extension().and_then(|e| e.to_str()) == Some("odt") {
            // ODT (OpenDocument Text) conversion pipeline
            let (body, _fm_config) = marksmen_core::config::frontmatter::parse_frontmatter(&markdown)?;
            let events = marksmen_core::parsing::parser::parse(body);
            let input_dir = input_path.parent().unwrap_or_else(|| std::path::Path::new("."));
            let odt_bytes = marksmen_odt::translate_and_render(&events, config, input_dir)?;
            
            fs::write(&out_path, &odt_bytes)
                .with_context(|| format!("Failed to write ODT output: {}", out_path.display()))?;
            
            tracing::info!(
                path = %out_path.display(),
                size_bytes = odt_bytes.len(),
                "ODT written"
            );
            return Ok(());
        } else {
            // PDF fallback
            write_pdf(&markdown, input_path, args, config)?;
        }
    } else {
        // Normal PDF conversion. (No explicit extension output provided)
        write_pdf(&markdown, input_path, args, config)?;
    }

    Ok(())
}

fn write_pdf(markdown: &str, input_path: &Path, args: &Args, config: &Config) -> Result<()> {
    let base_path = input_path.parent().map(|p| p.to_path_buf());
    let pdf_bytes = marksmen_pdf::convert(markdown, config, base_path)?;

    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| input_path.with_extension("pdf"));

    fs::write(&output_path, &pdf_bytes)
        .with_context(|| format!("Failed to write PDF output: {}", output_path.display()))?;

    tracing::info!(
        path = %output_path.display(),
        size_bytes = pdf_bytes.len(),
        "PDF written"
    );
    Ok(())
}
