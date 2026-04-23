# Marksmen Project Checklist

## Phase 8: Round-Trip Architectures and Format Expansion
- [x] Obtain user approval for round-trip parsing architecture
- [ ] Scaffold `marksmen-html` crate
- [ ] Implement `marksmen-html` AST translating to HTML5
- [ ] Implement MathML / KaTeX support for HTML equations
- [x] Scaffold `marksmen-docx-read` crate
- [x] Implement `marksmen-docx-read` Zip extraction and OOXML parsing
- [x] Implement `marksmen-docx-read` OMML mathematical equation restoration
- [x] Scaffold `marksmen-odt-read` crate
- [x] Implement `marksmen-odt-read` Zip extraction and ODF parsing
- [x] Scaffold `marksmen-pdf-read` crate and integrate mathematical extraction similarity
- [x] Scaffold `marksmen-roundtrip` testing suite
- [x] Build `demo.md` AST → Roundtrip → Extracted AST string similarity validation using `strsim`
- [x] Resolve structural payload divergence in DOCX abstract syntax tree translations
- [ ] Implement CLI integration for bidirectional formatting and output mapping

## Phase 9: PDF Annotation → DOCX Comment Localization (HIGH Severity)
- [x] Add `LocalizedAnnotation` type and `AnnotationSubtype` enum to `marksmen-pdf-read`
- [x] Implement `QuadPoints`, `Rect`, and color extraction from PDF annotations
- [x] Build `text_mapper.rs` glyph-position extractor (page coordinates → text ranges)
- [x] Integrate text mapper into annotation extraction loop (populate `anchored_text`)
- [x] Change markdown emission to wrap annotated text: `<mark>text</mark>`
- [x] Update `marksmen-pdf` comment injector to parse wrapped markdown tags
- [x] Implement Typst-layout-based or PDF-post-process position query for comment injection
- [x] Add `data-subtype` support to DOCX reader/writer (Highlight → `w:highlight`, Caret → `w:ins`, StrikeOut → `w:del`)
- [x] Add annotation count to `roundtrip_similarity` structural metric
- [x] Write property-based tests for PDF annotation roundtrip
- [x] Fix stale `marksmen-pdf/tests/roundtrip_test.rs` (references non-existent `translation::translator`)
- [x] Replace scratch `marksmen-pdf/tests/extract.rs` with real property tests
