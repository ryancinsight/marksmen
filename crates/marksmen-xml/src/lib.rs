//! Single source of truth for XML generation, serialization, and text escaping in the marksmen workspace.
//!
//! Encapsulates `quick-xml` escaping logic for abstract structural decoupling.

/// Escapes standard XML entities (`<`, `>`, `&`, `'`, `"`).
/// This method acts as the sole access point for string escaping without coupling downstream
/// elements directly to `quick-xml`'s API boundary.
pub fn escape(text: &str) -> String {
    quick_xml::escape::escape(text).into_owned()
}
