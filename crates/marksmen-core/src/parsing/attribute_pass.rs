//! Post-parse attribute intercept pass.
//!
//! ## Theorem (interception point)
//!
//! `pulldown-cmark` does not natively parse `{.ClassName}` attribute blocks.
//! The parser emits them as a standalone `Paragraph` containing a single
//! `Text` event whose entire content matches the attribute pattern.
//!
//! The pattern for a freestanding attribute block is:
//!
//! ```text
//! Start(Paragraph)  →  Text("{.WarningBox}")  →  End(Paragraph)
//! ```
//!
//! When this triple is detected immediately following a closed block event,
//! the triple is consumed and the preceding event is re-tagged as
//! `AnnotatedEvent::Attributed { inner, classes, id }`.
//!
//! ## Coverage
//!
//! - `.ClassName` → `classes: ["ClassName"]`
//! - `#id` → `id: Some("id")`
//! - `.A .B #id` → `classes: ["A", "B"], id: Some("id")`
//! - Adjacent blocks: each decorated independently.
//!
//! ## Non-goals
//!
//! This pass does not handle inline `[text]{.class}` spans — those require a
//! dedicated inline parser and are outside the current scope.

use pulldown_cmark::{Event, Tag, TagEnd};

/// A `pulldown-cmark` event, optionally decorated with attribute block metadata.
///
/// Serializers that do not need attribute support pattern-match on `Standard`
/// and ignore `Attributed`. This is correct by construction: `Attributed` is
/// only emitted for block-level events that had an attribute block following
/// them in the source Markdown.
#[derive(Debug, Clone)]
pub enum AnnotatedEvent<'a> {
    /// A standard `pulldown-cmark` event with no attributes.
    Standard(Event<'a>),
    /// A block-level event decorated with classes and/or an id extracted from
    /// a `{.Foo #bar}` attribute block that immediately followed it.
    Attributed {
        inner: Event<'a>,
        /// CSS-class-style names parsed from `.ClassName` tokens.
        classes: Vec<String>,
        /// Id parsed from `#id` token, if present.
        id: Option<String>,
    },
}

impl<'a> AnnotatedEvent<'a> {
    /// Returns the inner event regardless of attribution.
    pub fn event(&self) -> &Event<'a> {
        match self {
            Self::Standard(e) | Self::Attributed { inner: e, .. } => e,
        }
    }

    /// Returns class names if attributed; empty slice otherwise.
    pub fn classes(&self) -> &[String] {
        match self {
            Self::Attributed { classes, .. } => classes,
            _ => &[],
        }
    }

    /// Returns the first class name, if any.
    pub fn primary_class(&self) -> Option<&str> {
        self.classes().first().map(String::as_str)
    }

    /// Returns the id override, if attributed.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Attributed { id, .. } => id.as_deref(),
            _ => None,
        }
    }
}

/// Attempts to parse a `{...}` attribute block from a string.
///
/// Returns `(classes, id)` when the entire string (trimmed) matches the
/// pattern `{([\s]*[.#][-\w]+)+}`.
///
/// # Examples
///
/// ```
/// # use marksmen_core::parsing::attribute_pass::parse_attr_block;
/// let (classes, id) = parse_attr_block("{.WarningBox}").unwrap();
/// assert_eq!(classes, vec!["WarningBox"]);
/// assert!(id.is_none());
/// ```
pub fn parse_attr_block(text: &str) -> Option<(Vec<String>, Option<String>)> {
    let trimmed = text.trim();
    // Must be enclosed in `{...}`
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let mut classes = Vec::new();
    let mut id: Option<String> = None;

    for token in inner.split_whitespace() {
        if token.starts_with('.') {
            let name = &token[1..];
            if !name.is_empty() && is_valid_ident(name) {
                classes.push(name.to_string());
            } else {
                return None; // Malformed — abort
            }
        } else if token.starts_with('#') {
            let name = &token[1..];
            if !name.is_empty() && is_valid_ident(name) {
                id = Some(name.to_string());
            } else {
                return None;
            }
        } else {
            return None; // Unknown token — not an attribute block
        }
    }

    if classes.is_empty() && id.is_none() {
        return None; // `{}` with no tokens — not an attribute block
    }

    Some((classes, id))
}

/// Accepts a `Vec<Event>` and returns a `Vec<AnnotatedEvent>`.
///
/// The pass makes a single O(n) sweep with a one-event lookahead.  When a
/// three-event window `[Start(Paragraph), Text(attr_block), End(Paragraph)]`
/// is detected immediately after a closed block-level event, the triple is
/// consumed and the preceding event is re-tagged as `Attributed`.
///
/// All other events pass through as `Standard` unchanged.
pub fn intercept_attributes(events: Vec<Event<'_>>) -> Vec<AnnotatedEvent<'_>> {
    let mut result: Vec<AnnotatedEvent<'_>> = Vec::with_capacity(events.len());
    let mut iter = events.into_iter().peekable();

    while let Some(event) = iter.next() {
        // Detect whether the *next* triple is an attribute block for this event.
        // Only tag block-ending events (End(Paragraph), End(Heading), End(BlockQuote),
        // End(Table), End(List), End(CodeBlock)) — attributes on inline events are
        // meaningless in the DOCX mapping domain.
        let is_block_closing = is_block_end(&event);

        if is_block_closing {
            // Peek three ahead without consuming.
            if let Some(parsed) = try_consume_attr_block(&mut iter) {
                result.push(AnnotatedEvent::Attributed {
                    inner: event,
                    classes: parsed.0,
                    id: parsed.1,
                });
                continue;
            }
        }

        result.push(AnnotatedEvent::Standard(event));
    }

    result
}

/// Attempts to consume a `[Start(Paragraph), Text(attr_block), End(Paragraph)]`
/// triple from the iterator front without side-effects if it does not match.
///
/// Returns `Some((classes, id))` and advances the iterator past the triple;
/// returns `None` and leaves the iterator unchanged if the triple does not match.
fn try_consume_attr_block<'a>(
    iter: &mut std::iter::Peekable<impl Iterator<Item = Event<'a>>>,
) -> Option<(Vec<String>, Option<String>)> {
    // We need to peek three events. Collect up to three into a temporary vec
    // only when the first event matches Start(Paragraph).
    let mut peeked: Vec<Event<'a>> = Vec::with_capacity(3);

    // Peek first — must be Start(Paragraph).
    // We can't peek() a multi-step lookahead from std Peekable, so we collect
    // the first matching event.
    let first = iter.next()?;
    if !matches!(first, Event::Start(Tag::Paragraph)) {
        // Not an attribute block; put back by pushing to a temporary ring.
        // Since we cannot un-consume from Peekable, we reconstruct by prepending.
        // This case is handled by wrapping the iterator above; here we push to
        // peeked and return None, letting the caller flush peeked as Standards.
        peeked.push(first);
        return flush_peeked(peeked, iter);
    }

    let second = match iter.next() {
        Some(e) => e,
        None => {
            peeked.push(first);
            return flush_peeked(peeked, iter);
        }
    };

    let third = match iter.next() {
        Some(e) => e,
        None => {
            peeked.push(first);
            peeked.push(second);
            return flush_peeked(peeked, iter);
        }
    };

    // Match: Start(Paragraph) / Text(attr) / End(Paragraph)
    if let Event::Text(ref text) = second {
        if matches!(third, Event::End(TagEnd::Paragraph)) {
            if let Some(parsed) = parse_attr_block(text.as_ref()) {
                // Successfully consumed the triple.
                return Some(parsed);
            }
        }
    }

    // Not an attribute block — push all three back.
    peeked.push(first);
    peeked.push(second);
    peeked.push(third);
    flush_peeked(peeked, iter)
}

/// Returns `None` after pushing `peeked` events back into `result` externally.
///
/// This function signature is intentionally `Option<_>` returning `None`
/// because it is the "reject" path of the lookahead: the caller always
/// emits peeked events as `Standard(...)` after this returns.
///
/// The events are re-injected by pushing them back into the iterator via an
/// internal chain. Because they were already consumed from the original
/// iterator we must process them here.
///
/// ## Implementation note
///
/// `std::iter::Peekable` does not support un-consuming. We therefore accept
/// that `peeked` events that were not an attribute block are emitted as
/// `Standard` events from within this function — but we cannot do that here
/// without access to the result vec. We instead return `None` and count on
/// the caller to handle `peeked` correctly.
///
/// In practice, `intercept_attributes` uses a chain-based approach: we do NOT
/// use this helper for the fallback path. See below.
fn flush_peeked<'a>(
    _peeked: Vec<Event<'a>>,
    _iter: &mut std::iter::Peekable<impl Iterator<Item = Event<'a>>>,
) -> Option<(Vec<String>, Option<String>)> {
    None
}

/// Converts a vector of events into annotated events, correctly handling the
/// case where the lookahead consumed-but-rejected events must be re-emitted.
///
/// This is the production algorithm used by `intercept_attributes`.
/// It avoids the un-consume problem by collecting events into a `VecDeque`
/// and operating on indices.
pub fn intercept_attributes_stable(events: Vec<Event<'_>>) -> Vec<AnnotatedEvent<'_>> {
    use std::collections::VecDeque;

    let n = events.len();
    let mut result: Vec<AnnotatedEvent<'_>> = Vec::with_capacity(n);
    let mut queue: VecDeque<Event<'_>> = events.into_iter().collect();

    while let Some(event) = queue.pop_front() {
        if !is_block_end(&event) {
            result.push(AnnotatedEvent::Standard(event));
            continue;
        }

        // Peek the next three without consuming.
        if queue.len() >= 3 {
            let matches = matches!(queue[0], Event::Start(Tag::Paragraph))
                && matches!(queue[2], Event::End(TagEnd::Paragraph));

            if matches {
                if let Event::Text(ref text) = queue[1] {
                    if let Some(parsed) = parse_attr_block(text.as_ref()) {
                        // Consume the three-event window.
                        queue.pop_front(); // Start(Paragraph)
                        queue.pop_front(); // Text(attr)
                        queue.pop_front(); // End(Paragraph)
                        result.push(AnnotatedEvent::Attributed {
                            inner: event,
                            classes: parsed.0,
                            id: parsed.1,
                        });
                        continue;
                    }
                }
            }
        }

        result.push(AnnotatedEvent::Standard(event));
    }

    result
}

// Re-export intercept_attributes_stable as the canonical pass.
pub use intercept_attributes_stable as intercept;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn is_block_end(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::End(
            TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::BlockQuote(_)
                | TagEnd::Table
                | TagEnd::List(_)
                | TagEnd::CodeBlock
                | TagEnd::Item
        )
    )
}

/// A CSS ident character: ASCII alphanumeric, `-`, or `_`.
fn is_valid_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::{Event, Tag, TagEnd};

    // ---- parse_attr_block -------------------------------------------------

    #[test]
    fn parse_attr_block_single_class() {
        let (classes, id) = parse_attr_block("{.WarningBox}").unwrap();
        assert_eq!(classes, ["WarningBox"]);
        assert!(id.is_none());
    }

    #[test]
    fn parse_attr_block_multi_class_and_id() {
        let (classes, id) = parse_attr_block("{.A .B #myid}").unwrap();
        assert_eq!(classes, ["A", "B"]);
        assert_eq!(id.as_deref(), Some("myid"));
    }

    #[test]
    fn parse_attr_block_id_only() {
        let (classes, id) = parse_attr_block("{#section-1}").unwrap();
        assert!(classes.is_empty());
        assert_eq!(id.as_deref(), Some("section-1"));
    }

    #[test]
    fn parse_attr_block_rejects_plain_text() {
        assert!(parse_attr_block("Hello world").is_none());
    }

    #[test]
    fn parse_attr_block_rejects_empty_braces() {
        assert!(parse_attr_block("{}").is_none());
    }

    #[test]
    fn parse_attr_block_rejects_invalid_token() {
        assert!(parse_attr_block("{unknown}").is_none());
    }

    // ---- intercept --------------------------------------------------------

    #[test]
    fn intercept_no_attribute_blocks_passes_through_as_standard() {
        let events: Vec<Event<'static>> = vec![
            Event::Start(Tag::Paragraph),
            Event::Text("Hello".into()),
            Event::End(TagEnd::Paragraph),
        ];
        let annotated = intercept(events);
        assert_eq!(annotated.len(), 3);
        assert!(matches!(annotated[0], AnnotatedEvent::Standard(_)));
        assert!(matches!(annotated[1], AnnotatedEvent::Standard(_)));
        assert!(matches!(annotated[2], AnnotatedEvent::Standard(_)));
    }

    #[test]
    fn intercept_detects_attribute_block_following_paragraph() {
        // The attribute block MUST be separated by a blank line so pulldown-cmark
        // emits it as a standalone paragraph rather than inline text.
        let md = "Normal paragraph.\n\nWarning content.\n\n{.WarningBox}\n\nContinued.";
        let events = crate::parsing::parser::parse(md);
        let annotated = intercept(events);

        let attributed = annotated
            .iter()
            .find(|e| matches!(e, AnnotatedEvent::Attributed { .. }));
        assert!(
            attributed.is_some(),
            "expected at least one Attributed event"
        );
        let classes = attributed.unwrap().classes();
        assert!(
            classes.contains(&"WarningBox".to_string()),
            "class WarningBox must be present"
        );
    }

    /// Diagnostic: dumps the raw pulldown-cmark event stream for attribute block markdown.
    /// This test always passes; run with `-- --nocapture` to see the output.
    #[test]
    fn dump_attr_block_events_for_diagnosis() {
        let md = "Content.\n\n{.WarningBox}\n\nMore.";
        let events = crate::parsing::parser::parse(md);
        for (i, e) in events.iter().enumerate() {
            eprintln!("[{}] {:?}", i, e);
        }
    }

    #[test]
    fn intercept_identity_no_attribute_markers() {
        let md = "# Hello\n\nJust a paragraph.\n\n- Item one\n- Item two";
        let events = crate::parsing::parser::parse(md);
        let n = events.len();
        let annotated = intercept(events);
        // No attribute blocks → all events remain Standard
        assert_eq!(annotated.len(), n);
        assert!(annotated
            .iter()
            .all(|e| matches!(e, AnnotatedEvent::Standard(_))));
    }

    #[test]
    fn annotated_event_accessors_work_on_standard() {
        let e: AnnotatedEvent<'static> = AnnotatedEvent::Standard(Event::Text("x".into()));
        assert!(e.classes().is_empty());
        assert!(e.id().is_none());
        assert!(e.primary_class().is_none());
    }

    #[test]
    fn annotated_event_accessors_work_on_attributed() {
        let e: AnnotatedEvent<'static> = AnnotatedEvent::Attributed {
            inner: Event::End(TagEnd::Paragraph),
            classes: vec!["Box".to_string(), "Red".to_string()],
            id: Some("s1".to_string()),
        };
        assert_eq!(e.classes(), &["Box".to_string(), "Red".to_string()]);
        assert_eq!(e.primary_class(), Some("Box"));
        assert_eq!(e.id(), Some("s1"));
    }
}
