//! Configuration types for the conversion pipeline.
//!
//! Supports both programmatic configuration and YAML front-matter
//! parsed from the markdown document header.

pub mod frontmatter;
pub mod math;
pub mod page;
pub mod style_map;

use serde::{Deserialize, Serialize};

pub use math::MathConfig;
pub use page::PageConfig;
pub use style_map::StyleMap;

/// Top-level configuration for markdown-to-PDF conversion.
///
/// Fields can be overridden by YAML front-matter in the markdown document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Document title (used in PDF metadata and title page).
    #[serde(default)]
    pub title: String,

    /// Document author(s).
    #[serde(default)]
    pub author: String,

    /// Document date.
    #[serde(default)]
    pub date: String,

    /// Abstract text (for research articles).
    #[serde(rename = "abstract", default)]
    pub abstract_text: String,

    /// Page layout configuration.
    #[serde(default)]
    pub page: PageConfig,

    /// Math rendering configuration.
    #[serde(default)]
    pub math: MathConfig,

    /// Output destination path. If `None`, derived from the input path.
    #[serde(default)]
    pub dest: Option<String>,

    /// Code syntax highlight theme name. Default: `"github"`.
    #[serde(default = "default_highlight_theme")]
    pub highlight_theme: String,

    /// Optional path to a `.dotx` or `.docx` template for DOCX export.
    #[serde(default)]
    pub template_path: Option<String>,

    /// Optional PDF standard string (e.g. `pdf-a`) for Typst PDF export.
    #[serde(default)]
    pub pdf_standard: Option<String>,

    /// Global Word style mapping for DOCX output.
    ///
    /// When present, overrides the default structural style names emitted by
    /// `marksmen-docx` for headings, blockquotes, tables, code blocks, and
    /// body paragraphs. Populated from YAML front-matter `style_map:` block.
    #[serde(default)]
    pub style_map: StyleMap,

    /// Optional password for DOCX/PDF encryption (Stage 3 Security)
    #[serde(default)]
    pub password: Option<String>,

    /// Optional path to a PEM/DER X.509 certificate for PDF/DOCX digital signatures (Stage 4)
    #[serde(default)]
    pub certificate_path: Option<String>,

    /// Optional path to a PEM/DER private key for digital signatures
    #[serde(default)]
    pub private_key_path: Option<String>,
}

fn default_highlight_theme() -> String {
    "github".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            title: String::new(),
            author: String::new(),
            date: String::new(),
            abstract_text: String::new(),
            page: PageConfig::default(),
            math: MathConfig::default(),
            dest: None,
            highlight_theme: default_highlight_theme(),
            template_path: None,
            pdf_standard: None,
            style_map: StyleMap::default(),
            password: None,
            certificate_path: None,
            private_key_path: None,
        }
    }
}

impl Config {
    /// Merge front-matter overrides into this config, returning a new config.
    ///
    /// Front-matter values take precedence over existing config values
    /// when the front-matter field is explicitly set (non-default).
    pub fn merge_frontmatter(&self, fm: &FrontMatterConfig) -> Self {
        let mut merged = self.clone();
        if let Some(ref title) = fm.title {
            merged.title = title.clone();
        }
        if let Some(ref author) = fm.author {
            merged.author = author.clone();
        }
        if let Some(ref date) = fm.date {
            merged.date = date.clone();
        }
        if let Some(ref abstract_text) = fm.abstract_text {
            merged.abstract_text = abstract_text.clone();
        }
        if let Some(ref page) = fm.page {
            merged.page = page.clone();
        }
        if let Some(ref math) = fm.math {
            merged.math = math.clone();
        }
        if let Some(ref dest) = fm.dest {
            merged.dest = Some(dest.clone());
        }
        if let Some(ref theme) = fm.highlight_theme {
            merged.highlight_theme = theme.clone();
        }
        if let Some(ref tp) = fm.template_path {
            merged.template_path = Some(tp.clone());
        }
        if let Some(ref ps) = fm.pdf_standard {
            merged.pdf_standard = Some(ps.clone());
        }
        if let Some(ref sm) = fm.style_map {
            merged.style_map = sm.clone();
        }
        if let Some(ref pwd) = fm.password {
            merged.password = Some(pwd.clone());
        }
        merged
    }
}

/// Front-matter configuration parsed from YAML header.
///
/// All fields are optional — only explicitly set values override the base config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrontMatterConfig {
    pub title: Option<String>,
    pub author: Option<String>,
    pub date: Option<String>,
    #[serde(rename = "abstract")]
    pub abstract_text: Option<String>,
    pub page: Option<PageConfig>,
    pub math: Option<MathConfig>,
    pub dest: Option<String>,
    pub highlight_theme: Option<String>,
    pub template_path: Option<String>,
    pub pdf_standard: Option<String>,
    /// Global Word style map from YAML `style_map:` block.
    pub style_map: Option<StyleMap>,
    /// Document password for Agile Encryption.
    pub password: Option<String>,
}
