//! Data model for marksmen-cite.

use serde::{Deserialize, Serialize};

/// All reference types supported by the citation manager.
#[allow(dead_code)]
pub const REF_TYPES: &[&str] = &[
    "Journal Article",
    "Book",
    "Book Chapter",
    "Conference Paper",
    "Thesis",
    "Report",
    "Website",
    "Patent",
    "Preprint",
    "Other",
];

/// A single bibliographic reference record.
/// All new fields carry `#[serde(default)]` for backward compatibility
/// with existing `references.json` written by earlier schema versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub id: String,
    #[serde(default = "default_ref_type")]
    pub reference_type: String,
    pub title: String,
    pub authors: Vec<String>,
    pub abstract_text: String,
    pub journal: String,
    #[serde(default)]
    pub volume: String,
    #[serde(default)]
    pub issue: String,
    #[serde(default)]
    pub pages: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub edition: String,
    pub doi: String,
    pub pmid: String,
    #[serde(default)]
    pub isbn: String,
    #[serde(default)]
    pub issn: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub access_date: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub starred: bool,
    #[serde(default)]
    pub read_status: bool,
    pub pdf_path: Option<String>,
    pub year: String,
    #[serde(default)]
    pub date_added: String,
    #[serde(default)]
    pub date_modified: String,
    #[serde(default)]
    pub collections: Vec<String>,
}

fn default_ref_type() -> String {
    "Journal Article".to_string()
}

impl Reference {
    /// Construct a blank record with sensible defaults and current timestamps.
    pub fn blank() -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            reference_type: "Journal Article".to_string(),
            title: String::new(),
            authors: Vec::new(),
            abstract_text: String::new(),
            journal: String::new(),
            volume: String::new(),
            issue: String::new(),
            pages: String::new(),
            publisher: String::new(),
            edition: String::new(),
            doi: String::new(),
            pmid: String::new(),
            isbn: String::new(),
            issn: String::new(),
            url: String::new(),
            access_date: String::new(),
            language: String::new(),
            tags: Vec::new(),
            notes: String::new(),
            starred: false,
            read_status: false,
            pdf_path: None,
            year: String::new(),
            date_added: now.clone(),
            date_modified: now,
            collections: Vec::new(),
        }
    }

    /// Stamp the current UTC date into `date_modified`.
    #[allow(dead_code)]
    pub fn touch(&mut self) {
        self.date_modified = chrono::Utc::now().format("%Y-%m-%d").to_string();
    }
}

/// A named user collection grouping reference IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub ref_ids: Vec<String>,
}
