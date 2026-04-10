//! CLI module: argument parsing and orchestration.

pub mod runner;

use clap::Parser;
use std::path::PathBuf;

/// marksmen — Convert Markdown to PDF with native math equation support.
///
/// Supports `$...$` for inline math and `$$...$$` for display math using
/// Typst's math typesetting engine (LaTeX-quality output).
#[derive(Parser, Debug)]
#[command(name = "marksmen", version, about)]
pub struct Args {
    /// Input markdown file path(s). Supports glob patterns.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Output file path. If not specified, uses the input filename with .pdf extension.
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
}
