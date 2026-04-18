//! CLI module: argument parsing and orchestration.

pub mod runner;

use clap::Parser;
use std::path::PathBuf;

/// marksmen — Convert between Markdown and supported document formats.
///
/// Supports `md`, `docx`, `odt`, `html`, and `pdf` inputs. Output is inferred
/// from `--output` or defaults to `.pdf` for Markdown input and `.md` for
/// other source formats. `--as-typst` writes Typst source instead.
#[derive(Parser, Debug)]
#[command(name = "marksmen", version, about)]
pub struct Args {
    /// Input file path(s). Supported extensions: md, docx, odt, html, pdf.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Output file path. Extension determines the target format.
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,

    /// Disable math rendering.
    #[arg(long)]
    pub no_math: bool,

    /// Output as Typst source instead of PDF (for debugging).
    #[arg(long)]
    pub as_typst: bool,

    /// Watch input file(s) for changes and re-convert.
    #[arg(short = 'w', long)]
    pub watch: bool,

    /// Page width (e.g., "210mm", "8.5in").
    #[arg(long)]
    pub page_width: Option<String>,

    /// Page height (e.g., "297mm", "11in").
    #[arg(long)]
    pub page_height: Option<String>,

    /// Page margin (single value applied to all sides, e.g., "25mm").
    #[arg(long)]
    pub margin: Option<String>,

    /// Rasterize an input SVG file to a PNG file specified by --output.
    #[arg(long)]
    pub rasterize_svg: bool,

    /// Preprocess a Markdown file to replace Math with PNG links, writing to --output.
    #[arg(long)]
    pub preprocess_math: bool,
}
