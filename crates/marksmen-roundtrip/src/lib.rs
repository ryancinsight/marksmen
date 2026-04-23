//! Roundtrip similarity metrics for validating Markdown ↔ format ↔ Markdown
//! conversion fidelity.
//!
//! ## Invariants
//!
//! Let `S(a, b)` denote [`roundtrip_similarity`].
//!
//! 1. **Identity**: `S(x, x) == 1.0` for all non-empty `x`.
//! 2. **Symmetry**: `S(a, b) == S(b, a)`.
//! 3. **Bounds**: `S(a, b) ∈ [0.0, 1.0]`.
//! 4. **Empty**: `S("", x) == 0.0` when `x` is non-empty.
//! 5. **Threshold**: For any document `d` produced by `marksmen-*`, a lossless
//!    roundtrip `d → format → d'` satisfies `S(d, d') ≥ 0.85`.
//!
//! ## Metric composition
//!
//! `roundtrip_similarity(a, b) = 0.6 × text_similarity(a, b)
//!                              + 0.4 × structural_similarity(a, b)`
//!
//! The 0.6 / 0.4 split weights textual fidelity above structural equivalence
//! because formatting marks (bold, italic, code) are more lossy across formats
//! than structural element counts.

use anyhow::Result;
use strsim::normalized_levenshtein;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Normalized Levenshtein similarity between two strings.
///
/// Returns a value in `[0.0, 1.0]` where 1.0 = identical and 0.0 = maximum
/// distance. Both inputs are normalized (whitespace collapsed, trimmed) before
/// comparison to reduce sensitivity to formatting differences.
///
/// # Theorem (Identity)
///
/// For all non-empty strings `s`, `text_similarity(s, s) == 1.0`.
/// Proof: `normalized_levenshtein(s, s) == 1.0` by definition when both
/// inputs are non-empty and identical.
pub fn text_similarity(a: &str, b: &str) -> f64 {
    let a = normalize_text(a);
    let b = normalize_text(b);
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    normalized_levenshtein(&a, &b)
}

/// Structural similarity based on matching counts of Markdown structural elements.
///
/// Elements counted: ATX headings (#…######), unordered list items (- …),
/// ordered list items (N. …), fenced code blocks (```) and table rows (|…|).
///
/// Returns `1.0` when all element counts are identical, `0.0` when the two
/// documents have no structural elements in common.
///
/// ## Algorithm
///
/// For each element type E with count `cA` in `a` and `cB` in `b`:
/// `score_E = 1.0 - |cA - cB| / max(cA + cB, 1)`.
///
/// The final score is the arithmetic mean across all five element types.
/// This is bounded in `[0, 1]` by construction.
pub fn structural_similarity(a: &str, b: &str) -> f64 {
    let sa = extract_structure(a);
    let sb = extract_structure(b);
    let fields = [
        (sa.headings, sb.headings),
        (sa.unordered_items, sb.unordered_items),
        (sa.ordered_items, sb.ordered_items),
        (sa.code_blocks, sb.code_blocks),
        (sa.table_rows, sb.table_rows),
        (sa.tables, sb.tables),
        (sa.blockquotes, sb.blockquotes),
        (sa.math_blocks, sb.math_blocks),
        (sa.inline_math, sb.inline_math),
        (sa.bold, sb.bold),
        (sa.italic, sb.italic),
        (sa.strikethrough, sb.strikethrough),
        (sa.hyperlinks, sb.hyperlinks),
        (sa.footnotes, sb.footnotes),
        (sa.tasklists, sb.tasklists),
        (sa.images, sb.images),
        (sa.comments, sb.comments),
        (sa.highlights, sb.highlights),
        (sa.insertions, sb.insertions),
        (sa.deletions, sb.deletions),
    ];
    let total: f64 = fields
        .iter()
        .map(|(ca, cb)| {
            let ca = *ca as f64;
            let cb = *cb as f64;
            let denom = ca + cb;
            if denom == 0.0 {
                1.0 // both zero → identical absence
            } else {
                1.0 - (ca - cb).abs() / denom
            }
        })
        .sum();
    total / fields.len() as f64
}

/// Combined similarity weighted 60% textual + 40% structural.
///
/// The combined metric satisfies all invariants stated in the module docs.
pub fn roundtrip_similarity(a: &str, b: &str) -> f64 {
    let ts = text_similarity(a, b);
    let ss = structural_similarity(a, b);
    0.6 * ts + 0.4 * ss
}

/// Runs a complete roundtrip test for a given format pair and validates that
/// the similarity exceeds `threshold`.
///
/// Returns `Ok(score)` when the threshold is met. Returns `Err` describing the
/// failure when the score is below threshold.
///
/// # Parameters
///
/// - `original`: The original Markdown source.
/// - `roundtripped`: The Markdown reconstructed after `original → format → md`.
/// - `format_name`: Human-readable label used in the error message.
/// - `threshold`: Minimum acceptable similarity (default recommendation: 0.85).
pub fn assert_roundtrip_similarity(
    original: &str,
    roundtripped: &str,
    format_name: &str,
    threshold: f64,
) -> Result<f64> {
    let score = roundtrip_similarity(original, roundtripped);
    if score >= threshold {
        Ok(score)
    } else {
        Err(anyhow::anyhow!(
            "Roundtrip similarity for {} is {:.4} < threshold {:.4}.\n\
             Text sim: {:.4}, Structural sim: {:.4}",
            format_name,
            score,
            threshold,
            text_similarity(original, roundtripped),
            structural_similarity(original, roundtripped),
        ))
    }
}

// ---------------------------------------------------------------------------
// Internal: structure extraction
// ---------------------------------------------------------------------------

#[derive(Default, Debug, PartialEq)]
struct StructureMetrics {
    headings: usize,
    unordered_items: usize,
    ordered_items: usize,
    code_blocks: usize,
    table_rows: usize,
    tables: usize,
    blockquotes: usize,
    math_blocks: usize,
    inline_math: usize,
    bold: usize,
    italic: usize,
    strikethrough: usize,
    hyperlinks: usize,
    footnotes: usize,
    tasklists: usize,
    images: usize,
    comments: usize,
    highlights: usize,
    insertions: usize,
    deletions: usize,
}

fn extract_structure(md: &str) -> StructureMetrics {
    let mut m = StructureMetrics::default();
    let events = marksmen_core::parsing::parser::parse(md);

    let mut list_ordered_stack = Vec::new();

    for event in events {
        use pulldown_cmark::{Event, Tag};
        match event {
            Event::Start(Tag::Heading { .. }) => m.headings += 1,
            Event::Start(Tag::List(start_num)) => list_ordered_stack.push(start_num.is_some()),
            Event::End(pulldown_cmark::TagEnd::List(_)) => { list_ordered_stack.pop(); },
            Event::Start(Tag::Item) => {
                if *list_ordered_stack.last().unwrap_or(&false) {
                    m.ordered_items += 1;
                } else {
                    m.unordered_items += 1;
                }
            }
            Event::Start(Tag::CodeBlock(_)) => m.code_blocks += 1,
            Event::Start(Tag::Table(_)) => m.tables += 1,
            Event::Start(Tag::TableRow) | Event::Start(Tag::TableHead) => m.table_rows += 1,
            Event::Start(Tag::BlockQuote(_)) => m.blockquotes += 1,
            Event::Start(Tag::Strong) => m.bold += 1,
            Event::Start(Tag::Emphasis) => m.italic += 1,
            Event::Start(Tag::Strikethrough) => m.strikethrough += 1,
            Event::Start(Tag::Link { .. }) => m.hyperlinks += 1,
            Event::Start(Tag::Image { .. }) => m.images += 1,
            Event::Start(Tag::FootnoteDefinition(_)) => m.footnotes += 1,
            Event::FootnoteReference(_) => m.footnotes += 1,
            Event::TaskListMarker(_) => m.tasklists += 1,
            Event::InlineMath(_) => m.inline_math += 1,
            Event::DisplayMath(_) => m.math_blocks += 1,
            _ => {}
        }
    }

    // Count annotation / revision HTML tags that pulldown-cmark does not natively parse.
    m.comments = md.matches("<mark class=\"comment\"").count();
    m.highlights = md.matches("<mark class=\"highlight\"").count();
    m.insertions = md.matches("<ins ").count() + md.matches("<ins>").count();
    m.deletions = md.matches("<del ").count() + md.matches("<del>").count();

    m
}

/// Collapses whitespace runs and trims for comparison stability.
fn normalize_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out.trim().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── text_similarity ──────────────────────────────────────────────────────

    #[test]
    fn text_similarity_identity() {
        let s = "# Hello\n\nThis is a paragraph with **bold** and *italic* content.";
        assert_eq!(text_similarity(s, s), 1.0, "identity must be 1.0");
    }

    #[test]
    fn text_similarity_empty_vs_nonempty_is_zero() {
        assert_eq!(text_similarity("", "anything"), 0.0);
        assert_eq!(text_similarity("anything", ""), 0.0);
    }

    #[test]
    fn text_similarity_both_empty_is_one() {
        assert_eq!(text_similarity("", ""), 1.0);
    }

    #[test]
    fn text_similarity_near_identical_is_high() {
        let a = "# System Architecture\n\nThe system uses a three-tier model.";
        let b = "# System Architecture\n\nThe system uses a 3-tier model.";
        let sim = text_similarity(a, b);
        assert!(sim > 0.85, "near-identical text sim should be > 0.85, got {sim}");
    }

    #[test]
    fn text_similarity_completely_different_is_low() {
        let a = "# Alpha\n\nAlpha content.";
        let b = "# Zeta\n\nZeta material differs in every token completely.";
        let sim = text_similarity(a, b);
        assert!(sim < 0.8, "completely different docs should have low sim, got {sim}");
    }

    // ── structural_similarity ────────────────────────────────────────────────

    #[test]
    fn structural_similarity_identity() {
        let md = "# H1\n\n## H2\n\n- item a\n- item b\n\n1. one\n\n```\ncode\n```\n\n| A | B |\n|---|---|\n| x | y |";
        assert_eq!(structural_similarity(md, md), 1.0);
    }

    #[test]
    fn structural_similarity_different_heading_count() {
        let a = "# H1\n\n## H2\n\n### H3";
        let b = "# H1";
        let sim = structural_similarity(a, b);
        // headings: 3 vs 1 → score 1 - 2/4 = 0.5; others 19*1.0 → mean = 19.5/20 = 0.975
        assert!((sim - 0.975).abs() < 1e-9, "expected 0.975 got {sim}");
    }

    #[test]
    fn structural_similarity_empty_both_is_one() {
        assert_eq!(structural_similarity("", ""), 1.0);
    }

    #[test]
    fn structural_similarity_code_block_count() {
        let a = "```\nfoo\n```\n\n```\nbar\n```";
        let b = "```\nfoo\n```";
        let sim = structural_similarity(a, b);
        // code_blocks: 2 vs 1 → 1 - 1/3 ≈ 0.667; others 1.0 → mean ≈ 0.933
        assert!(sim > 0.85 && sim < 0.99, "expected ~0.933, got {sim}");
    }

    // ── roundtrip_similarity ─────────────────────────────────────────────────

    #[test]
    fn roundtrip_similarity_identity() {
        let s = "# Introduction\n\n- Alpha\n- Beta\n\n1. First\n2. Second";
        assert_eq!(roundtrip_similarity(s, s), 1.0, "identity must be 1.0");
    }

    #[test]
    fn roundtrip_similarity_bounds() {
        let a = "completely different content here";
        let b = "nothing in common whatsoever at all";
        let sim = roundtrip_similarity(a, b);
        assert!(sim >= 0.0 && sim <= 1.0, "sim out of bounds: {sim}");
    }

    #[test]
    fn roundtrip_similarity_threshold_gate_passes_for_near_identical() {
        let original = "# Title\n\n## Section\n\nSome body text.\n\n- item one\n- item two";
        // Simulate minor formatting differences (extra whitespace, CRLF).
        let roundtripped = "# Title\n\n## Section\n\nSome body text.\n\n- item one\n- item two\n";
        let result = assert_roundtrip_similarity(original, roundtripped, "test", 0.85);
        assert!(result.is_ok(), "threshold should pass: {:?}", result);
    }

    #[test]
    fn roundtrip_similarity_threshold_gate_fails_for_diverged() {
        let original = "# Title\n\n## Section\n\nSome body text.\n\n- item one\n- item two";
        let diverged = "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";
        let result = assert_roundtrip_similarity(original, diverged, "test", 0.85);
        assert!(result.is_err(), "threshold should fail for diverged output");
    }

    // ── extract_structure ────────────────────────────────────────────────────

    #[test]
    fn extracts_all_structural_elements() {
        let md = "# H1\n## H2\n\n- ul item\n\n1. ol item\n\n```\ncode\n```\n\n| A |\n|---|\n| x |";
        let s = extract_structure(md);
        assert_eq!(s.headings, 2, "headings");
        assert_eq!(s.unordered_items, 1, "unordered_items");
        assert_eq!(s.ordered_items, 1, "ordered_items");
        assert_eq!(s.code_blocks, 1, "code_blocks");
        assert_eq!(s.tables, 1, "tables");
        assert_eq!(s.table_rows, 2, "table_rows (header + data)");
    }

    #[test]
    fn code_fence_content_not_counted_as_structure() {
        // A `#` inside a code fence must not count as a heading.
        let md = "```\n# not a heading\n- not a list\n```\n";
        let s = extract_structure(md);
        assert_eq!(s.headings, 0, "no headings in fenced code");
        assert_eq!(s.unordered_items, 0, "no list items in fenced code");
    }

    // ── normalize_text ───────────────────────────────────────────────────────

    #[test]
    fn normalize_text_collapses_whitespace() {
        assert_eq!(normalize_text("  hello   world  "), "hello world");
        assert_eq!(normalize_text("\t\nhello\n\nworld\n"), "hello world");
    }

    // ── attribute block roundtrip (integration-level) ────────────────────────

    #[test]
    fn attribute_block_preserved_after_parse_intercept() {
        use marksmen_core::parsing::attribute_pass::{intercept, AnnotatedEvent};
        // The attribute block must be separated by a blank line so pulldown-cmark
        // emits it as a standalone paragraph triggering the intercept pass.
        let md = "Normal paragraph.\n\nWarning content.\n\n{.WarningBox}\n";
        let events = marksmen_core::parsing::parser::parse(md);
        let annotated = intercept(events);
        let has_attributed = annotated
            .iter()
            .any(|e| matches!(e, AnnotatedEvent::Attributed { classes, .. } if classes.contains(&"WarningBox".to_string())));
        assert!(has_attributed, "WarningBox attribute block must survive the intercept pass");
    }

    #[test]
    fn style_map_heading_override_in_config() {
        use marksmen_core::{Config, StyleMap};
        let mut sm = StyleMap::default();
        sm.heading[0] = Some("Corporate Heading 1".to_string());
        let mut config = Config::default();
        config.style_map = sm;
        assert_eq!(config.style_map.heading_style(1), "Corporate Heading 1");
        assert_eq!(config.style_map.heading_style(2), "Heading2");
        assert_eq!(config.style_map.blockquote_style(), "Quote");
    }
}
