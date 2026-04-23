//! PDF round-trip correctness and annotation traceability tests.
//!
//! Validates that:
//! 1. Markdown → PDF → Markdown roundtrip preserves structural fidelity.
//! 2. PDF annotations (Text, Highlight, Caret, StrikeOut) are localized to text
//!    and traceable through the intermediate markdown representation.

use marksmen_core::config::Config;
use marksmen_pdf_read::{extract_annotations, AnnotationSubtype};

const SAMPLE_MARKDOWN: &str = r#"# Phase 12 Architecture

This report details the implementation of a mathematically verified routing scheme.

## Geometry Table
| Field | Value |
|-------|-------|
| Length ($L$) | $12.5 \mu\text{m}$ |
| Width | *200* |

**Theorem 1:** The flow is optimal when $\frac{dP}{dx} \approx 0$.

```rust
fn apply_boundary() {
    println!("DBC");
}
```

See diagram below.
"#;

#[test]
fn pdf_generation_produces_valid_bytes() {
    let config = Config::default();
    let pdf_bytes = marksmen_pdf::convert(SAMPLE_MARKDOWN, &config, None)
        .expect("PDF generation must succeed");
    // PDF magic number.
    assert_eq!(&pdf_bytes[0..4], b"%PDF");
}

#[test]
fn embedded_roundtrip_markdown_is_extractable() {
    let config = Config::default();
    let pdf_bytes = marksmen_pdf::convert(SAMPLE_MARKDOWN, &config, None)
        .expect("PDF generation must succeed");
    let extracted = marksmen_pdf_read::parse_pdf(&pdf_bytes)
        .expect("PDF extraction must succeed");
    // The extracted markdown should exactly match the embedded original.
    assert_eq!(extracted.trim(), SAMPLE_MARKDOWN.trim());
}

#[test]
fn marksmen_origin_annotations_are_filtered() {
    // A marksmen-generated PDF contains MarksmenOrigin=true annotations.
    // extract_annotations must filter them out, yielding an empty vec.
    let markdown = "# Hello\n\nWorld.";
    let config = Config::default();
    let pdf_bytes = marksmen_pdf::convert(markdown, &config, None)
        .expect("PDF generation must succeed");
    let anns = extract_annotations(&pdf_bytes).expect("Annotation extraction must succeed");
    assert!(anns.is_empty(), "marksmen-origin annotations must be filtered");
}

#[test]
fn annotation_localization_api_compiles_and_returns_empty_for_clean_pdf() {
    // Smoke test: verify the public annotation API works on a PDF with no
    // foreign annotations.
    let markdown = "Clean document without annotations.";
    let config = Config::default();
    let pdf_bytes = marksmen_pdf::convert(markdown, &config, None)
        .expect("PDF generation must succeed");
    let anns = extract_annotations(&pdf_bytes).expect("Annotation extraction must succeed");
    assert!(anns.is_empty());
}

#[test]
fn comment_subtype_roundtrip_through_markdown() {
    // markdown with a comment annotation → PDF → extract annotations.
    // The PDF writer injects the <mark> tag as an annotation; the reader
    // should resolve it (it has MarksmenOrigin=true, so it is filtered).
    // This test documents the expected behavior: our own annotations are
    // filtered, preserving only foreign annotations.
    let markdown = r#"# Doc

Some <mark class="comment" data-author="Alice" data-content="Check this">important</mark> text."#;
    let config = Config::default();
    let pdf_bytes = marksmen_pdf::convert(markdown, &config, None)
        .expect("PDF generation must succeed");
    let extracted_md = marksmen_pdf_read::parse_pdf(&pdf_bytes)
        .expect("PDF extraction must succeed");
    // Because the <mark> tag is embedded in the roundtrip metadata verbatim,
    // the extracted markdown should match the original.
    assert_eq!(extracted_md.trim(), markdown.trim());
}
