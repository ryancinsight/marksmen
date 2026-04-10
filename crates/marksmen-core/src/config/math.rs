//! Math rendering configuration.

use serde::{Deserialize, Serialize};

/// Configuration for math equation rendering.
///
/// When enabled, `$...$` for inline math and `$$...$$` for display math
/// are rendered using Typst's native math typesetting engine. Typst math
/// supports a superset of common LaTeX math constructs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MathConfig {
    /// Whether math rendering is enabled. Default: `true`.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Default for MathConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
        }
    }
}
