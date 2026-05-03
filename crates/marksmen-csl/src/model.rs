use serde::{Deserialize, Serialize};

/// Represents a single citation reference conforming to CSL 1.0.2 variables.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct Reference {
    pub id: String,
    pub r#type: String, // "article-journal", "book", "chapter", etc.
    
    // Core String Variables
    pub title: Option<String>,
    pub container_title: Option<String>,
    pub publisher: Option<String>,
    pub publisher_place: Option<String>,
    pub page: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub isbn: Option<String>,
    pub issn: Option<String>,
    
    // Date Variables
    pub issued: Option<DateVariable>,
    pub accessed: Option<DateVariable>,
    
    // Name Variables
    pub author: Option<Vec<NameVariable>>,
    pub editor: Option<Vec<NameVariable>>,
    pub translator: Option<Vec<NameVariable>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DateVariable {
    #[serde(rename = "date-parts")]
    pub date_parts: Vec<Vec<i32>>, // e.g., [[2023, 10, 5]]
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct NameVariable {
    pub family: Option<String>,
    pub given: Option<String>,
    #[serde(rename = "dropping-particle")]
    pub dropping_particle: Option<String>,
    #[serde(rename = "non-dropping-particle")]
    pub non_dropping_particle: Option<String>,
    pub suffix: Option<String>,
    pub literal: Option<String>, // For institutional authors
}
