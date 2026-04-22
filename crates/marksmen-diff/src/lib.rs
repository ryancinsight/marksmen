//! Core diffing engine for `marksmen` workspace.
//!
//! Provides mathematically rigorous symmetric differences over strings, generating HTML `<ins>` and `<del>` endpoints.

use similar::{ChangeTag, TextDiff};

/// Derives the symmetric textual difference between two Markdown artifacts, generating semantic `<ins>` and `<del>` payloads.
pub fn diff_markdown(old: &str, new: &str) -> String {
    // We utilize word-level algorithmic alignment to match the visual semantics of Track Changes in word processors.
    let diff = TextDiff::from_words(old, new);
    let mut out = String::new();
    
    for change in diff.iter_all_changes() {
        let text = change.value();
        match change.tag() {
            ChangeTag::Delete => {
                // Ensure whitespace boundaries aren't incorrectly trapped inside deletion tags
                // if it disrupts markdown layout unnecessarily, but conceptually this matches exact bytes.
                out.push_str("<del>");
                out.push_str(text);
                out.push_str("</del>");
            }
            ChangeTag::Insert => {
                out.push_str("<ins>");
                out.push_str(text);
                out.push_str("</ins>");
            }
            ChangeTag::Equal => {
                out.push_str(text);
            }
        }
    }
    
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_insertion_and_deletion() {
        let old = "This is the original text.";
        let new = "This is the updated text.";
        let result = diff_markdown(old, new);
        assert_eq!(result, "This is the <del>original</del><ins>updated</ins> text.");
    }
}
