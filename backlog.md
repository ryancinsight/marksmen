# Marksmen Strategy Backlog

## Phase 12: Microsoft Office Online Clone Transition [ACTIVE]
- Strategy: Complete transition from basic markdown editor to an MS Office Online clone by stabilizing layout and implementing robust review/diff tools natively in Tauri.

## Phase 13: Feature Parity & Completeness Across Word, PDF, ODT (Next Increment)
- Strategy: Address identified formatting gaps to ensure total structural fidelity and format-agnostic rendering of advanced document elements (tables, images, field codes, nested tables).
- Target: Lossless roundtrip of advanced structures without silent degradations.

### Tactical Workstreams:
1. **Word (DOCX) Parity**: Implement robust multi-row/col mapping logic, precise field code mapping capabilities, and standard mathematical footnotes.
2. **Adobe (PDF) Parity**: Refine Typst geometric layout estimation matrices in extraction phase to identify true bounds for nested elements (Tables, Math) without relying solely on heuristic text size matching.
3. **OpenDocument (ODT) Parity**: Implement `draw:image` native ZIP blob embedding logic, standard `text:tracked-changes` markup for diffing tools, and advanced Table Grid span definitions.
