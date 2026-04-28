//! Shared Typst `World` implementation for the marksmen workspace.
//!
//! `MarksmenWorld` is the SSOT for Typst compiler configuration. Both
//! `marksmen-pdf` (PDF export) and `marksmen-render` (math PNG rasterization)
//! use the same world to guarantee identical font resolution and library
//! semantics.
//!
//! ## Theorem: Compilation Determinism
//! Given identical `typst_source` and system font set, `MarksmenWorld`
//! always produces byte-identical compilation output (modulo Typst's own
//! determinism guarantees, which hold for fixed source + fixed fonts).

use anyhow::{bail, Result};
use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

/// In-process Typst compiler world.
///
/// Provides the Typst compiler with:
/// - A virtual source file containing the Typst markup
/// - System fonts discovered at runtime
/// - The standard Typst library
pub struct MarksmenWorld {
    source: Source,
    main_id: FileId,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    library: LazyHash<Library>,
    base_path: std::path::PathBuf,
}

impl MarksmenWorld {
    /// Construct a world from Typst source markup and an optional base directory
    /// for resolving external asset references (images, etc.).
    pub fn new(typst_source: &str, base_path: Option<std::path::PathBuf>) -> Result<Self> {
        let main_id = FileId::new(None, VirtualPath::new("main.typ"));
        let source = Source::new(main_id, typst_source.to_string());
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
        std::fs::read(&path).map(Bytes::new).map_err(|e| {
            tracing::warn!(path = %path.display(), error = %e, "Failed to read asset");
            typst::diag::FileError::NotFound(id.vpath().as_rootless_path().into())
        })
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        Datetime::from_ymd(2026, 1, 1)
    }
}

// ── font discovery ─────────────────────────────────────────────────────────

fn discover_fonts() -> Result<(FontBook, Vec<Font>)> {
    let mut book = FontBook::new();
    let mut fonts = Vec::new();

    for dir in system_font_dirs() {
        if !dir.is_dir() {
            continue;
        }
        tracing::debug!(dir = %dir.display(), "Scanning font directory");
        for path in walkdir(&dir) {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
                continue;
            }
            let data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(e) => {
                    tracing::trace!(path = %path.display(), error = %e, "Skipping font");
                    continue;
                }
            };
            for font in Font::iter(Bytes::new(data)) {
                book.push(font.info().clone());
                fonts.push(font);
            }
        }
    }

    tracing::info!(font_count = fonts.len(), "Loaded system fonts");
    if fonts.is_empty() {
        bail!("No fonts found — ensure system fonts are installed.");
    }
    Ok((book, fonts))
}

fn system_font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "windows")]
    {
        if let Some(windir) = std::env::var_os("WINDIR") {
            dirs.push(std::path::PathBuf::from(windir).join("Fonts"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(
                std::path::PathBuf::from(local)
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
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(std::path::PathBuf::from(h).join("Library/Fonts"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push("/usr/share/fonts".into());
        dirs.push("/usr/local/share/fonts".into());
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(std::path::PathBuf::from(h).join(".local/share/fonts"));
        }
    }
    dirs
}

fn walkdir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                out.extend(walkdir(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}
