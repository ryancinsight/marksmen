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

    /// Global Word style mapping for DOCX output.
    ///
    /// When present, overrides the default structural style names emitted by
    /// `marksmen-docx` for headings, blockquotes, tables, code blocks, and
    /// body paragraphs. Populated from YAML front-matter `style_map:` block.
    #[serde(default)]
    pub style_map: StyleMap,
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
            style_map: StyleMap::default(),
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
        if let Some(ref sm) = fm.style_map {
            merged.style_map = sm.clone();
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
    /// Global Word style map from YAML `style_map:` block.
    pub style_map: Option<StyleMap>,
}
