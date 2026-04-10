//! marksmen — CLI tool for converting between Markdown and document formats.
//!
//! Uses the `marksmen` workspace conversion and reader crates to support
//! `Markdown <-> DOCX / ODT / HTML / PDF` style workflows through a
//! Markdown-like intermediate representation.

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
