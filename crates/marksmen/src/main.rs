//! marksmen — CLI tool for converting Markdown to PDF with native math support.
//!
//! Uses the `marksmen-core` library for the conversion pipeline:
//! `Markdown → pulldown-cmark → Typst → PDF`

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

mod cli;

fn main() -> Result<()> {
    // Initialize structured logging via tracing.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = cli::Args::parse();
    cli::runner::run(args)
}
