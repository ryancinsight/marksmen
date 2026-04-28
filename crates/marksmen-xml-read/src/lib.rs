//! Single source of truth for XML reading, event streaming, and AST translation.
//!
//! Exposes `quick-xml`'s zero-cost boundaries (Reader, Event) while enforcing
//! an abstraction shield to prevent upstream dependency drift.

// Expose zero-cost structurally identical mappings to maintain identical memory bounds
// for marksmen traversal architectures.
pub use quick_xml::escape::escape;
pub use quick_xml::events::Event;
pub use quick_xml::Reader;
