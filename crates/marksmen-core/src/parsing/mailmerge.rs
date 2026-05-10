use pulldown_cmark::{CowStr, Event, Tag};
use std::collections::HashMap;

/// Replaces `{{field}}` tags in a string with values from the given record.
/// Returns `Some(String)` if replacements occurred, otherwise `None`.
fn replace_tags(text: &str, record: &HashMap<String, String>) -> Option<String> {
    if !text.contains("{{") {
        return None;
    }
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut replaced_any = false;

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next(); // Consume second `{`
            let mut field_name = String::new();
            let mut found_close = false;

            while let Some(inner) = chars.next() {
                if inner == '}' && chars.peek() == Some(&'}') {
                    chars.next(); // Consume second `}`
                    found_close = true;
                    break;
                }
                field_name.push(inner);
            }

            if found_close {
                let key = field_name.trim();
                // Even if value is empty/missing, we consider it "replaced" as we removed the tag
                let val = record.get(key).map(|s| s.as_str()).unwrap_or("");
                output.push_str(val);
                replaced_any = true;
            } else {
                // Malformed tag, append what we consumed
                output.push_str("{{");
                output.push_str(&field_name);
            }
        } else {
            output.push(c);
        }
    }

    if replaced_any {
        Some(output)
    } else {
        None
    }
}

/// Applies variable substitution across an entire Markdown AST stream.
/// This operation is highly efficient: it clones unmodified AST events directly (O(1) reference copies)
/// and only allocates memory for text fragments containing replacement variables.
pub fn process_ast<'a>(
    template_events: &[Event<'a>],
    record: &HashMap<String, String>,
) -> Vec<Event<'a>> {
    let mut processed = Vec::with_capacity(template_events.len());

    for event in template_events {
        let new_event = match event {
            Event::Text(text) => {
                if let Some(new_text) = replace_tags(text, record) {
                    Event::Text(CowStr::Boxed(new_text.into_boxed_str()))
                } else {
                    event.clone()
                }
            }
            Event::Html(html) => {
                if let Some(new_html) = replace_tags(html, record) {
                    Event::Html(CowStr::Boxed(new_html.into_boxed_str()))
                } else {
                    event.clone()
                }
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let new_dest = replace_tags(dest_url, record)
                    .map(|s| CowStr::Boxed(s.into_boxed_str()))
                    .unwrap_or_else(|| dest_url.clone());
                let new_title = replace_tags(title, record)
                    .map(|s| CowStr::Boxed(s.into_boxed_str()))
                    .unwrap_or_else(|| title.clone());
                Event::Start(Tag::Link {
                    link_type: *link_type,
                    dest_url: new_dest,
                    title: new_title,
                    id: id.clone(),
                })
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let new_dest = replace_tags(dest_url, record)
                    .map(|s| CowStr::Boxed(s.into_boxed_str()))
                    .unwrap_or_else(|| dest_url.clone());
                let new_title = replace_tags(title, record)
                    .map(|s| CowStr::Boxed(s.into_boxed_str()))
                    .unwrap_or_else(|| title.clone());
                Event::Start(Tag::Image {
                    link_type: *link_type,
                    dest_url: new_dest,
                    title: new_title,
                    id: id.clone(),
                })
            }
            _ => event.clone(),
        };
        processed.push(new_event);
    }

    processed
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::{Event, Parser, Tag};

    #[test]
    fn test_ast_replacement() {
        let md = "Hello {{ First_Name }}! Your balance is {{Balance}}.";
        let mut record = HashMap::new();
        record.insert("First_Name".to_string(), "Alice".to_string());
        record.insert("Balance".to_string(), "$100".to_string());

        let events: Vec<_> = Parser::new(md).collect();
        let processed = process_ast(&events, &record);

        // AST should have the same structure but replaced text
        assert_eq!(processed.len(), events.len());
        if let Event::Text(ref text) = processed[1] {
            assert_eq!(text.as_ref(), "Hello Alice! Your balance is $100.");
        } else {
            panic!("Expected text node");
        }
    }

    #[test]
    fn test_missing_field_ast() {
        let md = "Hello {{Name}}";
        let record = HashMap::new();

        let events: Vec<_> = Parser::new(md).collect();
        let processed = process_ast(&events, &record);

        if let Event::Text(ref text) = processed[1] {
            assert_eq!(text.as_ref(), "Hello ");
        } else {
            panic!("Expected text node");
        }
    }
}
