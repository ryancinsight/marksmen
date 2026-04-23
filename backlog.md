# Marksmen Backlog & Strategy

## Architecture & Vision
- Enable bidirectional semantic conversion (Round-Trip) between analytical Markdown AST and visual binary formats (DOCX, ODT, PDF).
- Expand supported output domains (HTML5, RTF, EPUB).
- Maintain 100% pure Rust offline execution with mathematical correctness for all coordinate/bounds logic.

## Backlog Items
1. **[DONE] marksmen-html**: Output Markdown AST natively to HTML5.
2. **[DONE] marksmen-docx-read**: Extract structural semantics and Math vectors from raw .docx archives.
3. **[DONE] marksmen-odt-read**: Extract structural semantics and Math vectors from raw .odt archives.
4. **[DONE] marksmen-mermaid**: Expand Sugiyama implementation to support subgraphs and complex link rendering constraints.
5. **[DONE] PDF Annotation → DOCX Comment Localization**: Ensure PDF annotations (sticky notes, highlights, carets, strikeouts) are traceable to specific text ranges during PDF→DOCX conversion, with proper subtype semantics preserved.
6. **[DONE] Typst Roundtrip Text Degradation**: Fix `marksmen-typst-read` parser leaking preamble configuration (set rules, function calls, closures) into extracted markdown, causing roundtrip similarity to drop to ~0.52.

## Phase 9: PDF Annotation → DOCX Comment Localization (HIGH Severity)

### Sprint Goal
Close the traceability gap between PDF annotations and DOCX comments. A sticky note or highlight placed on text in a PDF must emerge as a comment anchored to that same text in the resulting DOCX, and vice versa.

### Strategy
- **Text-anchored intermediate representation**: Change the markdown `<mark>` tag from empty (`<mark class="comment" ...></mark>`) to text-wrapping (`<mark class="comment" ...>anchored text</mark>`). This aligns with how `marksmen-docx-read` already emits comments and enables position recovery.
- **Glyph-level text mapping in PDF reader**: Implement a `text_mapper` module that walks PDF content streams, tracks text matrices (`Tm`), and maps annotation `Rect`/`QuadPoints` to character ranges.
- **Typst-aware annotation injection in PDF writer**: Replace the byte-offset heuristic with layout-aware position injection, either by querying the Typst `Document` layout or post-processing the generated PDF.
- **Subtype-aware DOCX translation**: Map PDF Highlight → DOCX `w:highlight` + comment, Caret → `w:ins` (tracked insertion), StrikeOut → `w:del` (tracked deletion).

### Acceptance Criteria
- [ ] A PDF with a sticky note on "Status" produces a DOCX where the comment is anchored to the word "Status".
- [ ] A PDF with yellow highlighting over "Milestone 12" produces a DOCX where "Milestone 12" has both a comment and yellow highlighting.
- [ ] The `roundtrip_similarity` metric counts comments/annotations in structural similarity.
- [ ] Property tests verify that arbitrary markdown with `<mark class="comment">` tags roundtrips through PDF with anchored text preserved.
- [ ] All stale tests (`marksmen-pdf/tests/roundtrip_test.rs`, `marksmen-pdf/tests/extract.rs`) are replaced or removed.

### Dependencies
- `lopdf` (already in `marksmen-pdf-read`) for PDF dictionary parsing
- `pdf_extract` (already in `marksmen-pdf-read`) for text extraction — may need to fork or vendor for glyph-level access
- `typst` (already in `marksmen-pdf`) for layout queries — requires investigation of `typst::layout` API stability
