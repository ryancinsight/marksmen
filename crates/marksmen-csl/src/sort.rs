use crate::schema::{Sort, SortKey};
use crate::model::Reference;
use std::cmp::Ordering;

/// Compares two references based on the provided CSL Sort definition.
pub fn compare_references(a: &Reference, b: &Reference, sort: &Sort) -> Ordering {
    for key in &sort.keys {
        let (val_a, val_b) = extract_sort_value(a, b, key);
        
        let cmp = if let (Some(va), Some(vb)) = (&val_a, &val_b) {
            // Case-insensitive Unicode sorting is required by CSL
            va.to_lowercase().cmp(&vb.to_lowercase())
        } else if val_a.is_some() {
            Ordering::Greater
        } else if val_b.is_some() {
            Ordering::Less
        } else {
            Ordering::Equal
        };
        
        if cmp != Ordering::Equal {
            let sort_direction = key.sort.as_deref().unwrap_or("ascending");
            return if sort_direction == "descending" {
                cmp.reverse()
            } else {
                cmp
            };
        }
    }
    Ordering::Equal
}

fn extract_sort_value(a: &Reference, b: &Reference, key: &SortKey) -> (Option<String>, Option<String>) {
    if let Some(var) = &key.variable {
        let val_a = match var.as_str() {
            "title" => a.title.clone(),
            "author" => a.author.as_ref().and_then(|authors| {
                authors.first().and_then(|name| name.family.clone())
            }),
            "issued" => a.issued.as_ref().and_then(|date| {
                date.date_parts.first().and_then(|parts| parts.first().map(|y| y.to_string()))
            }),
            _ => None,
        };
        
        let val_b = match var.as_str() {
            "title" => b.title.clone(),
            "author" => b.author.as_ref().and_then(|authors| {
                authors.first().and_then(|name| name.family.clone())
            }),
            "issued" => b.issued.as_ref().and_then(|date| {
                date.date_parts.first().and_then(|parts| parts.first().map(|y| y.to_string()))
            }),
            _ => None,
        };
        
        return (val_a, val_b);
    }
    
    if let Some(_macro_name) = &key.macro_name {
        // TODO: In a complete engine, we would invoke the macro evaluator
        // on both references, strip the HTML/Markup, and compare the raw text strings.
        // For zero-cost strictness, this requires passing the Context into the sort engine.
        return (None, None);
    }
    
    (None, None)
}
