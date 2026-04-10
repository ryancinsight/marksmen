//! Round-trip correctness testing for the extracted marksmen-pdf crate.
//!
//! This test acts as a mathematical lock. A complex markdown payload
//! is fed through the frontend parser and passed into the `marksmen-pdf`
//! translator. We assert that the resulting Typst source exactly matches
//! the known canonical output string generated prior to the crate split,
//! guaranteeing zero functional degradation.

use marksmen_core::config::Config;
use marksmen_core::parsing::parser::parse;
use marksmen_pdf::translation::translator::translate;

const SAMPLE_MARKDOWN: &str = r#"
# Phase 12 Architecture

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

![Diagram](path/to/diagram.png)
"#;

#[test]
fn test_roundtrip_translation_lossless() {
    let mut config = Config::default();
    config.math.enabled = true;
    config.page.page_numbers = true;
    
    // Parse the frontend AST.
    let events = parse(SAMPLE_MARKDOWN);
    
    // Pass AST to the isolated marksmen-pdf engine.
    let typst_source = translate(events, &config).expect("Translation failed");
    
    // The exact structural output expected. Update this snapshot by inspecting the `left`
    // side of a failing assertion after intentional translator changes.
    let expected_typst = r#"#set page(width: 210mm, height: 297mm, margin: (top: 30mm, right: 25mm, bottom: 30mm, left: 25mm),
  numbering: "1",
)
#set text(font: "Arial", size: 11pt)
#show raw: set text(font: ("Consolas", "Courier New", "monospace"), size: 10pt)
#show raw.where(block: false): it => highlight(fill: luma(245), extent: 1.5pt)[#it]
#set heading(numbering: none)
#set par(justify: true)


= Phase 12 Architecture

This report details the implementation of a mathematically verified routing scheme.

== Geometry Table

#align(center)[
#table(
  columns: 2,
  inset: 3pt,
  align: (auto, auto),
  [ *Field* ],
  [ *Value* ],
  [ Length ($L$) ],
  [ $12.5 mu"m"$ ],
  [ Width ],
  [ #emph[200] ],
)
]

#strong[Theorem 1:] The flow is optimal when $frac(dP, dx) approx 0$.

```rust
fn apply_boundary() {
    println!("DBC");
}

```


#image("path/to/diagram.png")
Diagram"#;

    // Use a diff-friendly assertion.
    assert_eq!(
        typst_source.trim(),
        expected_typst.trim(),
        "The generated Typst source does not precisely match the documented baseline!"
    );
}
