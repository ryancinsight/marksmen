//! CLI runner: orchestrates file reading, conversion, and output writing.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use marksmen_core::Config;

use super::Args;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Markdown,
    Docx,
    Odt,
    Html,
    Xhtml,
    Pdf,
    Typst,
}

/// Run the conversion process based on CLI arguments.
pub fn run(args: Args) -> Result<()> {
    let mut config = Config::default();

    if args.no_math {
        config.math.enabled = false;
    }

    if args.rasterize_svg {
        return run_rasterize_svg(&args);
    }
    if args.preprocess_math {
        return run_preprocess_math(&args);
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

fn run_rasterize_svg(args: &Args) -> Result<()> {
    if args.files.is_empty() {
        bail!("--rasterize-svg requires an input file");
    }
    let input_path = &args.files[0];
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| input_path.with_extension("png"));

    tracing::info!(input = %input_path.display(), output = %output_path.display(), "Rasterizing SVG to PNG");
    let svg_bytes = fs::read(input_path).context("Failed to read SVG file")?;

    if let Some((png_bytes, _, _)) = marksmen_render::svg_bytes_to_png(&svg_bytes) {
        fs::write(&output_path, png_bytes).context("Failed to write PNG file")?;
        tracing::info!("Created {}", output_path.display());
    } else {
        bail!("Failed to rasterize SVG into PNG");
    }
    Ok(())
}

fn run_preprocess_math(args: &Args) -> Result<()> {
    if args.files.is_empty() {
        bail!("--preprocess-math requires an input markdown file");
    }
    let input_path = &args.files[0];
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| input_path.with_extension("md"));

    tracing::info!(input = %input_path.display(), output = %output_path.display(), "Preprocessing Markdown math to PNGs");
    let source = fs::read_to_string(input_path).context("Failed to read Markdown file")?;

    let input_dir = input_path.parent().unwrap_or_else(|| Path::new(""));
    let math_dir = input_dir.join("diagrams").join("math");
    fs::create_dir_all(&math_dir).context("Failed to create diagrams/math directory")?;

    let mut result_md = String::with_capacity(source.len());
    let mut eq_index = 0;

    // We do a simple line-by-line parsing to avoid mutating the structure of the AST.
    // Display Math block: lines starting with `$$` or surrounded by `$$`.
    let mut in_display_math = false;
    let mut current_math = String::new();
    let mut display_math_lines = 0;

    for line in source.lines() {
        let tline = line.trim();
        if in_display_math {
            if tline.ends_with("$$") || tline == "$$" {
                // Closing display math
                if tline != "$$" {
                    // Extract before the $$
                    current_math.push_str(&line[..line.len() - 2]);
                }
                eq_index += 1;
                let trimmed = current_math.trim();
                let math_png_path = math_dir.join(format!("eq_{}.png", eq_index));
                if let Some((png_bytes, _, _)) = marksmen_render::render_math_to_png(trimmed, true)
                {
                    fs::write(&math_png_path, png_bytes)?;
                } else {
                    tracing::warn!("Failed to render eq_{} (display)", eq_index);
                }
                let rel_path = math_png_path
                    .strip_prefix(input_dir)
                    .unwrap_or(&math_png_path);
                result_md.push_str(&format!(
                    "![Equation {}]({})\n",
                    eq_index,
                    rel_path.display().to_string().replace('\\', "/")
                ));
                in_display_math = false;
            } else {
                current_math.push_str(line);
                current_math.push('\n');
                display_math_lines += 1;
            }
            continue;
        }

        if tline.starts_with("$$") {
            let rest = &tline[2..];
            if rest.ends_with("$$") && !rest.is_empty() {
                // Single line: $$ ... $$
                let eq_str = &rest[..rest.len() - 2].trim();
                eq_index += 1;
                let math_png_path = math_dir.join(format!("eq_{}.png", eq_index));
                if let Some((png_bytes, _, _)) = marksmen_render::render_math_to_png(eq_str, true) {
                    fs::write(&math_png_path, png_bytes)?;
                } else {
                    tracing::warn!("Failed to render eq_{} (inline-display)", eq_index);
                }
                let rel_path = math_png_path
                    .strip_prefix(input_dir)
                    .unwrap_or(&math_png_path);
                result_md.push_str(&format!(
                    "![Equation {}]({})\n",
                    eq_index,
                    rel_path.display().to_string().replace('\\', "/")
                ));
            } else if rest == "" {
                in_display_math = true;
                current_math.clear();
                display_math_lines = 0;
            } else {
                in_display_math = true;
                current_math.clear();
                current_math.push_str(rest);
                current_math.push('\n');
            }
            continue;
        }

        // Inline math parsing `$ ... $`
        let mut out_line = String::new();
        let mut prev = 0;
        let bytes = line.as_bytes();
        let mut s = 0;
        while s < bytes.len() {
            if bytes[s] == b'$' {
                if s + 1 < bytes.len() && bytes[s + 1] == b'$' {
                    out_line.push_str(&line[prev..s + 2]);
                    prev = s + 2;
                    s += 2;
                    continue;
                }
                let start = s + 1;
                let mut end = start;
                while end < bytes.len() && !(bytes[end] == b'$' && bytes[end - 1] != b'\\') {
                    end += 1;
                }
                if end >= bytes.len() {
                    s += 1;
                    continue;
                }
                let latex = &line[start..end];
                out_line.push_str(&line[prev..s]);
                eq_index += 1;
                let math_png_path = math_dir.join(format!("eq_{}.png", eq_index));
                if let Some((png_bytes, _, _)) = marksmen_render::render_math_to_png(latex, false) {
                    fs::write(&math_png_path, png_bytes)?;
                } else {
                    tracing::warn!("Failed to render inline math: {}", latex);
                }
                let rel_path = math_png_path
                    .strip_prefix(input_dir)
                    .unwrap_or(&math_png_path);
                out_line.push_str(&format!(
                    "![]({})",
                    rel_path.display().to_string().replace('\\', "/")
                ));
                prev = end + 1;
                s = end + 1;
            } else {
                s += 1;
            }
        }
        out_line.push_str(&line[prev..]);
        result_md.push_str(&out_line);
        result_md.push('\n');
    }

    fs::write(&output_path, result_md).context("Failed to write annotated Markdown file")?;
    tracing::info!("Preprocessed math into {} equations", eq_index);
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

    // Read raw source bytes for DOCX→DOCX roundtrip style preservation.
    // For all other conversion paths this is unused.
    let source_docx_bytes: Option<Vec<u8>> =
        if source_format == Format::Docx && target_format == Format::Docx {
            fs::read(input_path).ok()
        } else {
            None
        };

    let markdown = read_as_markdown_like(input_path, source_format, &output_path)?;
    write_output(
        &markdown,
        input_path,
        &output_path,
        target_format,
        config,
        source_docx_bytes.as_deref(),
    )
}

fn read_as_markdown_like(
    input_path: &Path,
    source_format: Format,
    output_path: &Path,
) -> Result<String> {
    match source_format {
        Format::Markdown => fs::read_to_string(input_path)
            .with_context(|| format!("Failed to read input file: {}", input_path.display())),
        Format::Typst => {
            let typst_source = fs::read_to_string(input_path).with_context(|| {
                format!("Failed to read Typst input file: {}", input_path.display())
            })?;
            marksmen_typst_read::parse_typst(&typst_source)
                .with_context(|| format!("Failed to parse Typst input: {}", input_path.display()))
        }
        Format::Html => {
            let html = fs::read_to_string(input_path)
                .with_context(|| format!("Failed to read HTML input: {}", input_path.display()))?;
            marksmen_html_read::parse_html(&html)
                .with_context(|| format!("Failed to parse HTML input: {}", input_path.display()))
        }
        Format::Xhtml => {
            let xhtml = fs::read_to_string(input_path)
                .with_context(|| format!("Failed to read XHTML input: {}", input_path.display()))?;
            marksmen_xhtml_read::parse_xhtml(&xhtml)
                .with_context(|| format!("Failed to parse XHTML input: {}", input_path.display()))
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
            marksmen_odt_read::parse_odt(&bytes, None)
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
    source_docx_bytes: Option<&[u8]>,
) -> Result<()> {
    match target_format {
        Format::Markdown => {
            fs::write(output_path, markdown).with_context(|| {
                format!("Failed to write Markdown output: {}", output_path.display())
            })?;
            tracing::info!(path = %output_path.display(), "Markdown written");
            Ok(())
        }
        Format::Typst => {
            let (body, fm_config) =
                marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
            let merged = config.merge_frontmatter(&fm_config);
            let events = marksmen_core::parsing::parser::parse(body);
            let typst_source = marksmen_typst::translator::translate(events, &merged)?;

            fs::write(output_path, &typst_source).with_context(|| {
                format!("Failed to write Typst output: {}", output_path.display())
            })?;
            tracing::info!(path = %output_path.display(), "Typst source written");
            Ok(())
        }
        Format::Html => {
            let (body, fm_config) =
                marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
            let merged = config.merge_frontmatter(&fm_config);
            let events = marksmen_core::parsing::parser::parse(body);
            let html = marksmen_html::convert(events, &merged)?;

            fs::write(output_path, &html).with_context(|| {
                format!("Failed to write HTML output: {}", output_path.display())
            })?;
            tracing::info!(path = %output_path.display(), size_bytes = html.len(), "HTML written");
            Ok(())
        }
        Format::Xhtml => {
            let (body, fm_config) =
                marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
            let merged = config.merge_frontmatter(&fm_config);
            let events = marksmen_core::parsing::parser::parse(body);
            let xhtml = marksmen_xhtml::convert(events, &merged)?;

            fs::write(output_path, &xhtml).with_context(|| {
                format!("Failed to write XHTML output: {}", output_path.display())
            })?;
            tracing::info!(path = %output_path.display(), size_bytes = xhtml.len(), "XHTML written");
            Ok(())
        }
        Format::Docx => {
            let (body, fm_config) =
                marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
            let merged = config.merge_frontmatter(&fm_config);
            let events = marksmen_core::parsing::parser::parse(body);
            let input_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
            let docx_bytes = marksmen_docx::translation::document::convert(
                events,
                &merged,
                input_dir,
                source_docx_bytes,
            )?;

            fs::write(output_path, &docx_bytes).with_context(|| {
                format!("Failed to write DOCX output: {}", output_path.display())
            })?;
            tracing::info!(path = %output_path.display(), size_bytes = docx_bytes.len(), "DOCX written");
            Ok(())
        }
        Format::Odt => {
            let (body, fm_config) =
                marksmen_core::config::frontmatter::parse_frontmatter(markdown)?;
            let merged = config.merge_frontmatter(&fm_config);
            let events = marksmen_core::parsing::parser::parse(body);
            let input_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
            let odt_bytes = marksmen_odt::translate_and_render(&events, &merged, input_dir)?;

            fs::write(output_path, &odt_bytes).with_context(|| {
                format!("Failed to write ODT output: {}", output_path.display())
            })?;
            tracing::info!(path = %output_path.display(), size_bytes = odt_bytes.len(), "ODT written");
            Ok(())
        }
        Format::Pdf => {
            let base_path = input_path.parent().map(|p| p.to_path_buf());
            let pdf_bytes = marksmen_pdf::convert(markdown, config, base_path)?;

            fs::write(output_path, &pdf_bytes).with_context(|| {
                format!("Failed to write PDF output: {}", output_path.display())
            })?;
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
    infer_format_from_path(path)
        .with_context(|| format!("Unsupported input format: {}", path.display()))
}

fn infer_output_format(path: &Path, as_typst: bool) -> Result<Format> {
    if as_typst {
        return Ok(Format::Typst);
    }
    infer_format_from_path(path)
        .with_context(|| format!("Unsupported output format: {}", path.display()))
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
        "xhtml" | "xht" => Ok(Format::Xhtml),
        "pdf" => Ok(Format::Pdf),
        "typ" => Ok(Format::Typst),
        _ => bail!("unsupported extension: .{}", ext),
    }
}

#[cfg(test)]
mod tests {
    use super::{infer_format_from_path, Format};
    use std::path::Path;

    #[test]
    fn infers_supported_extensions() {
        assert_eq!(
            infer_format_from_path(Path::new("test.md")).unwrap(),
            Format::Markdown
        );
        assert_eq!(
            infer_format_from_path(Path::new("test.docx")).unwrap(),
            Format::Docx
        );
        assert_eq!(
            infer_format_from_path(Path::new("test.odt")).unwrap(),
            Format::Odt
        );
        assert_eq!(
            infer_format_from_path(Path::new("test.html")).unwrap(),
            Format::Html
        );
        assert_eq!(
            infer_format_from_path(Path::new("test.xhtml")).unwrap(),
            Format::Xhtml
        );
        assert_eq!(
            infer_format_from_path(Path::new("test.xht")).unwrap(),
            Format::Xhtml
        );
        assert_eq!(
            infer_format_from_path(Path::new("test.pdf")).unwrap(),
            Format::Pdf
        );
        assert_eq!(
            infer_format_from_path(Path::new("test.typ")).unwrap(),
            Format::Typst
        );
    }
}
