use pulldown_cmark::{CowStr, Event, Tag};

/// Concatenates multiple Markdown AST event streams into a single unified stream,
/// namespacing IDs to prevent cross-reference collisions.
pub struct AstConcatenator<'a> {
    events: Vec<Event<'a>>,
}

impl<'a> Default for AstConcatenator<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> AstConcatenator<'a> {
    pub fn new() -> Self {
        Self {
            events: Vec::with_capacity(1024),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
        }
    }

    /// Adds a document's AST to the combined stream, namespacing its IDs.
    /// A page break is inserted before the document if it is not the first one.
    pub fn add_document(
        &mut self,
        namespace: &str,
        doc_events: impl IntoIterator<Item = Event<'a>>,
    ) -> &mut Self {
        let iter = doc_events.into_iter();
        let (lower, upper) = iter.size_hint();
        let capacity_hint = upper.unwrap_or(lower);

        self.events.reserve(capacity_hint + 1);

        if !self.events.is_empty() {
            // Insert a page break
            self.events.push(Event::Html(CowStr::Borrowed(
                "<div style=\"page-break-after: always;\"></div>\n",
            )));
        }

        for event in iter {
            let namespaced_event = match event {
                // Namespace Heading IDs (e.g. {#my-heading} -> {#namespace-my-heading})
                Event::Start(Tag::Heading {
                    level,
                    id,
                    classes,
                    attrs,
                }) => {
                    let new_id =
                        id.map(|i| CowStr::Boxed(format!("{}-{}", namespace, i).into_boxed_str()));
                    Event::Start(Tag::Heading {
                        level,
                        id: new_id,
                        classes,
                        attrs,
                    })
                }

                // Namespace Link references (e.g. [Link](#my-heading) -> [Link](#namespace-my-heading))
                Event::Start(Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                }) => {
                    let new_url = if let Some(stripped) = dest_url.strip_prefix('#') {
                        CowStr::Boxed(format!("#{}-{}", namespace, stripped).into_boxed_str())
                    } else {
                        dest_url
                    };
                    Event::Start(Tag::Link {
                        link_type,
                        dest_url: new_url,
                        title,
                        id,
                    })
                }

                // Namespace raw HTML IDs and HREFs if they exist in the text (basic heuristic)
                Event::Html(html) => {
                    let mut s = html.into_string();
                    s = s.replace("id=\"", &format!("id=\"{}-", namespace));
                    s = s.replace("href=\"#", &format!("href=\"#{}-", namespace));
                    Event::Html(CowStr::Boxed(s.into_boxed_str()))
                }

                other => other,
            };
            self.events.push(namespaced_event);
        }

        self
    }

    /// Returns the combined AST stream.
    pub fn build(self) -> Vec<Event<'a>> {
        self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::{Options, Parser};

    #[test]
    fn test_ast_concatenator() {
        let md1 = "# Chapter 1 {#ch1}\n[Go to figure](#fig1)\n<div id=\"fig1\">Figure</div>";
        let md2 = "# Chapter 2 {#ch2}\n[Go to figure](#fig1)\n<div id=\"fig1\">Figure 2</div>";

        let mut options = Options::empty();
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_SUPERSCRIPT);
        options.insert(Options::ENABLE_SUBSCRIPT);
        let p1: Vec<_> = Parser::new_ext(md1, options).collect();
        let p2: Vec<_> = Parser::new_ext(md2, options).collect();

        let mut concat = AstConcatenator::new();
        concat.add_document("doc1", p1);
        concat.add_document("doc2", p2);

        let combined = concat.build();

        // Assert page break
        assert!(combined
            .iter()
            .any(|e| matches!(e, Event::Html(h) if h.contains("page-break"))));

        // Assert namespaced headers
        assert!(combined.iter().any(|e| matches!(e, Event::Start(Tag::Heading { id: Some(i), .. }) if i.as_ref() == "doc1-ch1")));
        assert!(combined.iter().any(|e| matches!(e, Event::Start(Tag::Heading { id: Some(i), .. }) if i.as_ref() == "doc2-ch2")));

        // Assert namespaced links
        assert!(combined.iter().any(|e| matches!(e, Event::Start(Tag::Link { dest_url, .. }) if dest_url.as_ref() == "#doc1-fig1")));
        assert!(combined.iter().any(|e| matches!(e, Event::Start(Tag::Link { dest_url, .. }) if dest_url.as_ref() == "#doc2-fig1")));

        // Assert namespaced HTML ids
        assert!(combined
            .iter()
            .any(|e| matches!(e, Event::Html(h) if h.contains("id=\"doc1-fig1\""))));
        assert!(combined
            .iter()
            .any(|e| matches!(e, Event::Html(h) if h.contains("id=\"doc2-fig1\""))));
    }
}
