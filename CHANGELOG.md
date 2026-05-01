# Changelog

All notable changes to the Marksmen workspace will be documented in this file.

## [1.1.0] - 2026-05-01

### Added
- **Marksmen Cite Integration**: Deployed a fully functional citation manager as a native Tauri application (`marksmen-cite`) with local database storage, matching feature-parity with Mendeley and EndNote.
- **Crossref Web Automation**: Implemented automated web-fetching of citation metadata (Title, Authors, Abstract, Year) natively resolving via Crossref DOIs.
- **Legacy Database Migration**: Built zero-cost native parsers for `.ris` and `.bib` formats, enabling drag-and-drop legacy database ingestion via system file dialogs.
- **Deep PDF Data Extraction**: Refactored `marksmen-pdf-read` to bypass missing metadata dictionary headers by deeply parsing raw PDF text geometry to locate and resolve DOIs autonomously.
- **Database Deduplication**: Deployed a linear-time deduplication engine to safely find and merge redundant citation entries.

### Fixed
- **Windows Linker Limitations**: Mitigated GNU linker panics on Windows MSYS2 cross-compilation by correctly configuring static linkage profiles and pruning excessive `cdylib` dynamic export thresholds.
- **Citation Rendering**: Fixed right-pane detail rendering regressions and fully restored dark-mode glassmorphic aesthetics.

## [1.0.0] - 2026-04-28

### Added
- **Bidirectional Format Parity**: Full roundtrip parsing and translation capabilities across DOCX, ODT, PDF, and HTML formats, ensuring high structural fidelity and no silent formatting degradations.
- **Tauri Editor Migration**: Transitioned from a basic markdown text shell to a fully featured MS Office Online clone natively powered by Tauri, complete with a structured Ribbon UI, Print Layout pagination, and offline IPC payload handling.
- **Native Tracked Changes**: Natively parse and emit `<text:tracked-changes>` for OpenDocument Text (ODT), allowing true review tooling capabilities directly in the target application.
- **DOCX Field Codes**: Resolved structural gaps preventing field codes and deep references from natively evaluating during bidirectional roundtrips.
- **Native PDF Bounding Matrices**: Precise geometric bounding boxes extracted from Typst layouts natively to reconstruct exact tables, matrices, and paragraph limits without external heuristic hints.
- **SVG Vector Diagrams**: Deployed `usvg` + `resvg` zero-allocation image pipeline natively within `marksmen-mermaid` to natively convert Sugiyama layouts into portable PNG / SVG elements cleanly inside generated assets.

### Changed
- **Zero-Allocation Kernels**: Migrated Apollo computational kernels and text routing to strictly utilize `Arc` reference counting and zero-copy slicing for optimal speed.
- **Strict Diagnostics**: Cleared all internal compiler, `clippy`, and unused code scaffolding from the workspace. Achieved a zero-regression, zero-warning test suite compilation state.

### Removed
- **Legacy axum paths**: Pulled the standalone web server capabilities (`marksmen-webui`) out in favor of the Tauri desktop application strategy.
- **Heuristic Layout Hacks**: Deleted naive font-size heuristic algorithms from `marksmen-pdf-read` in favor of precise matrix transformation bounds calculations.
