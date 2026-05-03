# Changelog

All notable changes to the Marksmen workspace will be documented in this file.

## [1.2.0] - 2026-05-03

### Added
- **WebAssembly Extensibility (`marksmen-plugin`)**: Deployed a zero-cost, sandboxed Wasm plugin engine leveraging `wasmtime`, enabling users to inject custom JS/Go/Rust AST filters equivalent to Pandoc's Lua architecture.
- **Enterprise Legal Compliance**: Implemented strict Data Loss Prevention (DLP) across rendering pipelines. Introduced destructive redaction (`<redact>`) that mathematically strips sensitive byte data from the Typst and DOCX generators, replacing it with unrecoverable geometric blocks.
- **Interactive Form Synthesis**: Bridged structural HTML forms (`<form>`) into native interactive Word Content Controls (`FORMTEXT`) and PDF visual bounding box markers.
- **Cloud Synchronization & LWW Merge**: Finalized the `marksmen-cite` synchronization engine with a deterministic Last-Writer-Wins (LWW) resolution engine for handling divergent remote `references.json` databases across WebDAV/S3 limits.
- **Browser Ingestion Extension**: Fully deployed a Chrome/Firefox extension that actively scrapes `citation_doi` metadata from academic journal portals and proxies it directly to the local `marksmen-cite` Axum server.
- **AST Assembly Pipelines**: The `export_binder` compiler efficiently maps and namespaces unified references and footnotes across merged multi-chapter documents in O(n) memory.

### Changed
- **Zero-Warning Strict Compiler Sweep**: Cleared all remaining `unused_mut`, `dead_code`, and redundant generic warnings across `marksmen-html`, `marksmen-crypto`, and translation engines.
- **Optimized Memory Profiling**: Eradicated unused state heap-inflation from translation tracking structs. `AstConcatenator` now enforces aggressive memory capacity allocations (`Vec::with_capacity(1024)`) during large-scale document bundling to eliminate OS-level fragmentations.

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
