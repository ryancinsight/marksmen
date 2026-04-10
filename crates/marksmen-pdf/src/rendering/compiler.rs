//! Typst compilation and PDF export.
//!
//! This module provides an in-process Typst compiler that takes generated
//! Typst markup, compiles it to a `PagedDocument`, and exports it as PDF.
//!
//! ## Architecture
//!
//! The `MarksmenWorld` struct implements `typst::World`, providing:
//! - A virtual source file containing the generated Typst markup
//! - System fonts discovered at runtime
//! - The standard Typst library
//!
//! ## Theorem: Compilation Determinism
//!
//! Given identical Typst source and font set, the compiler always produces
//! byte-identical PDF output (modulo PDF creation timestamps, which are
//! set to a fixed value).


use anyhow::{Result, bail};
use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};

// LibraryExt provides `Library::default()`.
use typst::LibraryExt;

use marksmen_core::config::Config;

/// Compile Typst source markup to PDF bytes.
///
/// This is the terminal step in the conversion pipeline. It:
/// 1. Creates a `MarksmenWorld` with the source and system fonts
/// 2. Invokes the Typst compiler to layout the document
/// 3. Exports the laid-out pages to PDF bytes
pub fn compile_to_pdf(typst_source: &str, _config: &Config, base_path: Option<std::path::PathBuf>) -> Result<Vec<u8>> {
    let world = MarksmenWorld::new(typst_source, base_path)?;

    // Compile the Typst source into a document.
    let document = typst::compile(&world)
        .output
        .map_err(|diagnostics| {
            let messages: Vec<String> = diagnostics
                .iter()
                .map(|d| {
                    let mut loc = String::new();
                    if let Some(id) = d.span.id() {
                        if let Ok(src) = world.source(id) {
                            if let Some(range) = src.range(d.span) {
                                let start = range.start.saturating_sub(40);
                                let end = (range.end + 40).min(src.text().len());
                                let context = &src.text()[start..end];
                                loc = format!(" near `{:?}`", context);
                            }
                        }
                    }
                    format!("{:?}{}: {}", d.severity, loc, d.message)
                })
                .collect();
            anyhow::anyhow!("Typst compilation failed:\n{}", messages.join("\n"))
        })?;

    // Export to PDF.
    let pdf_bytes = typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|errs| {
            let messages: Vec<String> = errs
                .iter()
                .map(|e| format!("{:?}", e))
                .collect();
            anyhow::anyhow!("PDF export failed:\n{}", messages.join("\n"))
        })?;

    Ok(pdf_bytes)
}

/// Typst World implementation for marksmen.
///
/// Provides the Typst compiler with everything it needs:
/// - Source file (the generated Typst markup)
/// - Font book (system fonts)
/// - Standard library
struct MarksmenWorld {
    /// The main source file.
    source: Source,
    /// The file ID for the main source.
    main_id: FileId,
    /// Font book for lookup (wrapped in LazyHash for the World trait).
    book: LazyHash<FontBook>,
    /// Loaded fonts.
    fonts: Vec<Font>,
    /// The standard Typst library (wrapped in LazyHash for the World trait).
    library: LazyHash<Library>,
    /// Base directory for resolving external assets (like images).
    base_path: std::path::PathBuf,
}

impl MarksmenWorld {
    fn new(typst_source: &str, base_path: Option<std::path::PathBuf>) -> Result<Self> {
        let main_id = FileId::new(None, VirtualPath::new("main.typ"));
        let source = Source::new(main_id, typst_source.to_string());

        // Discover and load system fonts.
        let (book, fonts) = discover_fonts()?;

        Ok(Self {
            source,
            main_id,
            book: LazyHash::new(book),
            fonts,
            library: LazyHash::new(Library::default()),
            base_path: base_path.unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
        })
    }
}

impl World for MarksmenWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main_id {
            Ok(self.source.clone())
        } else {
            Err(typst::diag::FileError::NotFound(
                id.vpath().as_rootless_path().into(),
            ))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = self.base_path.join(id.vpath().as_rootless_path());
        std::fs::read(&path)
            .map(|data| Bytes::new(data))
            .map_err(|e| {
                tracing::warn!(path = %path.display(), error = %e, "Failed to read external asset");
                typst::diag::FileError::NotFound(id.vpath().as_rootless_path().into())
            })
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        // Fixed timestamp for deterministic output.
        Datetime::from_ymd(2026, 1, 1)
    }
}

/// Discover system fonts and build a font book.
///
/// Searches platform-specific font directories:
/// - Windows: `C:\Windows\Fonts`, local app data fonts
/// - macOS: `/Library/Fonts`, `~/Library/Fonts`
/// - Linux: `/usr/share/fonts`, `~/.local/share/fonts`
fn discover_fonts() -> Result<(FontBook, Vec<Font>)> {
    let mut book = FontBook::new();
    let mut fonts = Vec::new();

    // Collect font directories.
    let font_dirs = get_system_font_dirs();

    for dir in &font_dirs {
        if !dir.is_dir() {
            continue;
        }

        tracing::debug!(dir = %dir.display(), "Scanning font directory");

        for path in walkdir(dir) {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if !matches!(ext.to_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc") {
                continue;
            }

            let data = match std::fs::read(&path) {
                Ok(data) => data,
                Err(e) => {
                    tracing::trace!(path = %path.display(), error = %e, "Skipping unreadable font");
                    continue;
                }
            };

            let bytes = Bytes::new(data);

            for font in Font::iter(bytes) {
                book.push(font.info().clone());
                fonts.push(font);
            }
        }
    }

    tracing::info!(font_count = fonts.len(), "Loaded system fonts");

    if fonts.is_empty() {
        bail!("No fonts found. Ensure system fonts are installed.");
    }

    Ok((book, fonts))
}

/// Get platform-specific font directories.
fn get_system_font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Some(windir) = std::env::var_os("WINDIR") {
            dirs.push(std::path::PathBuf::from(windir).join("Fonts"));
        }
        if let Some(localappdata) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(
                std::path::PathBuf::from(localappdata)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Fonts"),
            );
        }
    }

    #[cfg(target_os = "macos")]
    {
        dirs.push("/Library/Fonts".into());
        dirs.push("/System/Library/Fonts".into());
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(std::path::PathBuf::from(home).join("Library/Fonts"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        dirs.push("/usr/share/fonts".into());
        dirs.push("/usr/local/share/fonts".into());
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(std::path::PathBuf::from(home).join(".local/share/fonts"));
        }
    }

    dirs
}

/// Simple recursive directory walk (avoids adding walkdir as a dependency).
fn walkdir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.extend(walkdir(&path));
            } else {
                results.push(path);
            }
        }
    }
    results
}
