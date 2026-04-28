//! Page layout configuration.
//!
//! Controls the physical dimensions and margins of the generated PDF.

use serde::{Deserialize, Serialize};

/// Page layout configuration for the PDF output.
///
/// ## Defaults
///
/// - **Size**: A4 (210mm × 297mm)
/// - **Margins**: top=30mm, right=25mm, bottom=30mm, left=25mm
/// - **Orientation**: Portrait
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageConfig {
    /// Page width (e.g., `"210mm"`, `"8.5in"`).
    #[serde(default = "default_width")]
    pub width: String,

    /// Page height (e.g., `"297mm"`, `"11in"`).
    #[serde(default = "default_height")]
    pub height: String,

    /// Top margin.
    #[serde(default = "default_margin_vertical")]
    pub margin_top: String,

    /// Right margin.
    #[serde(default = "default_margin_horizontal")]
    pub margin_right: String,

    /// Bottom margin.
    #[serde(default = "default_margin_vertical")]
    pub margin_bottom: String,

    /// Left margin.
    #[serde(default = "default_margin_horizontal")]
    pub margin_left: String,

    /// Optional header content.
    pub header: Option<String>,

    /// Optional footer content.
    pub footer: Option<String>,

    /// Enable automatic page numbering if footer is not explicitly set.
    #[serde(default)]
    pub page_numbers: bool,

    /// Default font size for the document.
    pub font_size: Option<String>,

    /// Default font family for the document.
    pub font_family: Option<String>,

    /// Default line/paragraph spacing modifier.
    pub line_spacing: Option<String>,
}

fn default_width() -> String {
    "210mm".to_string()
}

fn default_height() -> String {
    "297mm".to_string()
}

fn default_margin_vertical() -> String {
    "30mm".to_string()
}

fn default_margin_horizontal() -> String {
    "25mm".to_string()
}

impl Default for PageConfig {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            margin_top: default_margin_vertical(),
            margin_right: default_margin_horizontal(),
            margin_bottom: default_margin_vertical(),
            margin_left: default_margin_horizontal(),
            header: None,
            footer: None,
            page_numbers: false,
            font_size: None,
            font_family: None,
            line_spacing: None,
        }
    }
}
