//! CLI runner: orchestrates file reading, conversion, and output writing.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use marksmen_core::Config;

use super::Args;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Markdown,
    Docx,
    Odt,
    Html,
    Pdf,
    Typst,
}

/// Run the conversion process based on CLI arguments.
pub fn run(args: Args) -> Result<()> {
    let mut config = Config::default();

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

    if args.files.len() > 1 && args.output.is_some() {
        bail!("--output can only be used with a single input file");
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

fn convert_file(input_path: &Path, args: &Args, config: &Config) -> Result<()> {
    let source_format = infer_input_format(input_path)?;
    let output_path = determine_output_path(input_path, args, source_format)?;
    let target_format = infer_output_format(&output_path, args.as_typst)?;

    tracing::info!(
        path = %input_path.display(),
        source = ?source_format,
        target = ?target_format,
        "Converting"
    );

    let markdown = read_as_markdown_like(input_path, source_format, &output_path)?;
    write_output(&markdown, input_path, &output_path, target_format, config)
}

fn read_as_markdown_like(input_path: &Path, source_format: Format, output_path: &Path) -> Result<String> {
    match source_format {
        Format::Markdown | Format::Typst => fs::read_to_string(input_path)
            .with_context(|| format!("Failed to read input file: {}", input_path.display())),
        Format::Html => {
            let html = fs::read_to_string(input_path)
                .with_context(|| format!("Failed to read HTML input: {}", input_path.display()))?;
            marksmen_html_read::parse_html(&html)
                .with_context(|| format!("Failed to parse HTML input: {}", input_path.display()))
        }
        Format::Docx => {
            let bytes = fs::read(input_path)
                .with_context(|| format!("Failed to read DOCX input: {}", input_path.display()))?;
            
            let mut media_dir = None;
            let mut media_path_buf = PathBuf::new();
            if let Some(file_stem) = output_path.file_stem() {
                let parent = output_path.parent().unwrap_or_else(|| Path::new(""));
                let dir_name = format!("{}_media", file_stem.to_string_lossy());
                media_path_buf = parent.join(dir_name);
                if let Err(e) = fs::create_dir_all(&media_path_buf) {
                    tracing::warn!("Failed to create media directory: {}", e);
                } else {
                    media_dir = Some(media_path_buf.as_path());
                }
            }

            marksmen_docx_read::parse_docx(&bytes, media_dir)
                .with_context(|| format!("Failed to parse DOCX input: {}", input_path.display()))
        }
        Format::Odt => {
            let bytes = fs::read(input_path)
                .with_context(|| format!("Failed to read ODT input: {}", input_path.display()))?;
            marksmen_odt_read::parse_odt(&bytes)
                .with_context(|| format!("Failed to parse ODT input: {}", input_path.display()))
        }
        Format::Pdf => {
            let bytes = fs::read(input_path)
                .with_context(|| format!("Failed to read PDF input: {}", input_path.display()))?;
            marksmen_pdf_read::parse_pdf(&bytes)
                .with_context(|| format!("Failed to parse PDF input: {}", input_path.display()))
        }
    }
}

fn write_output(
    markdown: &str,
    input_path: &Path,
    output_path: &Path,
    target_format: Format,
    config: &Config,
) -> Result<()> {
    match target_format {
        Format::Markdown => {
            fs::write(output_path, markdown)
                .with_context(|| format!("Failed to write Markdown output: {}", output_path.display()))?;
            tracing::info!(path = %output_path.display(), "Markdown written");
            Ok(())
        }
        Format::Typst => {
            let (body, fm_config) = marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
            let merged = config.merge_frontmatter(&fm_config);
            let events = marksmen_core::parsing::parser::parse(body);
            let typst_source = marksmen_pdf::translation::translator::translate(events, &merged)?;

            fs::write(output_path, &typst_source)
                .with_context(|| format!("Failed to write Typst output: {}", output_path.display()))?;
            tracing::info!(path = %output_path.display(), "Typst source written");
            Ok(())
        }
        Format::Html => {
            let (body, fm_config) = marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
            let merged = config.merge_frontmatter(&fm_config);
            let events = marksmen_core::parsing::parser::parse(body);
            let html = marksmen_html::convert(events, &merged)?;

            fs::write(output_path, &html)
                .with_context(|| format!("Failed to write HTML output: {}", output_path.display()))?;
            tracing::info!(path = %output_path.display(), size_bytes = html.len(), "HTML written");
            Ok(())
        }
        Format::Docx => {
            let (body, fm_config) = marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
            let merged = config.merge_frontmatter(&fm_config);
            let events = marksmen_core::parsing::parser::parse(body);
            let input_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
            let docx_bytes = marksmen_docx::translation::document::convert(events, &merged, input_dir)?;

            fs::write(output_path, &docx_bytes)
                .with_context(|| format!("Failed to write DOCX output: {}", output_path.display()))?;
            tracing::info!(path = %output_path.display(), size_bytes = docx_bytes.len(), "DOCX written");
            Ok(())
        }
        Format::Odt => {
            let (body, fm_config) = marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
            let merged = config.merge_frontmatter(&fm_config);
            let events = marksmen_core::parsing::parser::parse(body);
            let input_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
            let odt_bytes = marksmen_odt::translate_and_render(&events, &merged, input_dir)?;

            fs::write(output_path, &odt_bytes)
                .with_context(|| format!("Failed to write ODT output: {}", output_path.display()))?;
            tracing::info!(path = %output_path.display(), size_bytes = odt_bytes.len(), "ODT written");
            Ok(())
        }
        Format::Pdf => {
            let base_path = input_path.parent().map(|p| p.to_path_buf());
            let pdf_bytes = marksmen_pdf::convert(markdown, config, base_path)?;

            fs::write(output_path, &pdf_bytes)
                .with_context(|| format!("Failed to write PDF output: {}", output_path.display()))?;
            tracing::info!(path = %output_path.display(), size_bytes = pdf_bytes.len(), "PDF written");
            Ok(())
        }
    }
}

fn determine_output_path(input_path: &Path, args: &Args, source_format: Format) -> Result<PathBuf> {
    if let Some(out_path) = &args.output {
        return Ok(out_path.clone());
    }

    if args.as_typst {
        return Ok(input_path.with_extension("typ"));
    }

    let default_extension = match source_format {
        Format::Markdown => "pdf",
        _ => "md",
    };

    Ok(input_path.with_extension(default_extension))
}

fn infer_input_format(path: &Path) -> Result<Format> {
    let format = infer_format_from_path(path).with_context(|| format!("Unsupported input format: {}", path.display()))?;
    if format == Format::Typst {
        bail!("Typst input is not supported: {}", path.display());
    }
    Ok(format)
}

fn infer_output_format(path: &Path, as_typst: bool) -> Result<Format> {
    if as_typst {
        return Ok(Format::Typst);
    }
    infer_format_from_path(path).with_context(|| format!("Unsupported output format: {}", path.display()))
}

fn infer_format_from_path(path: &Path) -> Result<Format> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .context("missing file extension")?;

    match ext.as_str() {
        "md" | "markdown" => Ok(Format::Markdown),
        "docx" => Ok(Format::Docx),
        "odt" => Ok(Format::Odt),
        "html" | "htm" => Ok(Format::Html),
        "pdf" => Ok(Format::Pdf),
        "typ" => Ok(Format::Typst),
        _ => bail!("unsupported extension: .{}", ext),
    }
}

#[cfg(test)]
mod tests {
    use super::{Format, infer_format_from_path};
    use std::path::Path;

    #[test]
    fn infers_supported_extensions() {
        assert_eq!(infer_format_from_path(Path::new("test.md")).unwrap(), Format::Markdown);
        assert_eq!(infer_format_from_path(Path::new("test.docx")).unwrap(), Format::Docx);
        assert_eq!(infer_format_from_path(Path::new("test.odt")).unwrap(), Format::Odt);
        assert_eq!(infer_format_from_path(Path::new("test.html")).unwrap(), Format::Html);
        assert_eq!(infer_format_from_path(Path::new("test.pdf")).unwrap(), Format::Pdf);
        assert_eq!(infer_format_from_path(Path::new("test.typ")).unwrap(), Format::Typst);
    }
}
