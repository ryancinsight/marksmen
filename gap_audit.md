# Marksmen Gap Audit

## Current Identified Gaps
- **[CLOSED] Missing HTML Output Layer**: `marksmen-html` has been successfully implemented with analytical Markdown AST projection and inline Mermaid SVG mapping, delivering strict zero-dependency HTML5 outputs.

## Resolved Gaps
- **[CLOSED] Missing Bidirectional Extraction**: `marksmen-docx`, `marksmen-odt`, and `marksmen-pdf` now have pure-rust bidirectional evaluation crates (`-read`) enforcing similarity.
- **[CLOSED] DOCX Formatting Degration**: Mitigated 13KB AST formatting loss by synchronizing text-run flushes (`has_runs`) against Pulldown-cmark block boundaries, eliminating newline swallowing.
- **[CLOSED] Mathematical Typesetting**: Supported in Typst, DOCX, and ODT natively through analytical mapping.
- **[CLOSED] Pure-Rust Vector Graphics**: Mermaid flowcharts successfully bypass web DOM measurements through the internal Sugiyama layout core.
