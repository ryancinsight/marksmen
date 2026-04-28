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

## Phase 10: Parity Testing & Issue Correction
- [x] Fix Typst roundtrip degradation (preamble `FuncCall`/`Closure` leakage into markdown)
- [x] Fix `marksmen-odt-read` display math handling (`P_DisplayMath` without paired `P_HiddenMeta`)
- [x] Fix `marksmen-typst` `inline_math_translation` test (outdated expectation for `latex_to_typst` spacing)
- [x] Fix `marksmen-core` doc-test (outdated `convert` module reference)
- [x] Implement global page-margin `global_min_x` normalization to stabilize inter-page layout alignment bounds
- [x] Enforce list-typo padding normalization bounds to snap `< 3x body_size` misaligned ordered bullets to zero-padding origins
- [x] Full workspace test suite green (unit + integration + roundtrip_demo + doc-tests)

## Phase 11: Tauri Desktop Migration
- [x] Scaffold `marksmen-editor` Tauri Application inside workspace.
- [x] Implement native IPC backend translating local strings to generic Markdown AST outputs.
- [x] Inject DOM conversion events translating `contenteditable` tags natively into `marksmen-html-read` compatible semantic elements (`<b>` to `strong`, `<i>` to `em`).
- [x] Remove legacy networked axum payload paths (`marksmen-webui`).

## Phase 12: Microsoft Office Online Clone Transition
- [x] Overhaul CSS styling to implement simulated Word paginated "Print Layout" and formal Ribbon structure.
- [x] Wire structural HTML Ribbon toggles bridging OS Dialog Pickers to `marksmen-*_read` ingestion.
- [x] Attach inline DOM insertion `<mark class="comment">` wrapper events to replicate standard Office Annotations.
- [x] Plumb `marksmen-diff` through to Track Changes visualization lockdown node in frontend.

## Phase 13: Format Feature Parity & Completeness
- [ ] Evaluate DOCX AST missing mappings for dynamic field codes and footnotes.
- [ ] Implement DOCX field-code to Markdown mapping bridge.
- [ ] Design true `draw:image` embedding pipeline for ODT translation.
- [ ] Refactor ODT table generation to support nested tables and grid spans.
- [ ] Resolve PDF coordinate bounds extraction to match complex Typst-compiled geometric layouts.
- [ ] Integrate complex review/diff rendering natively across ODT tracked changes (`<text:tracked-changes>`).
