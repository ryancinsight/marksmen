# Marksmen Strategy Backlog

## Phase 18: Full Stack Citation Manager Deployment [COMPLETE]
- Strategy: Complete transition of `marksmen-cite` from an MVP to a production-ready, feature-complete citation database matching EndNote/Mendeley.
- Target: Lossless web fetching, legacy DB parsers, zero-cost architecture.

### Tactical Workstreams:
1. **Automated Crossref Resolution**: Fully deployed deep PDF geometry extraction and `reqwest` DOI fetching for zero-input metadata hydration.
2. **Legacy Import Strategy**: Built and deployed `.ris` and `.bib` native format parsers to safely onboard legacy library users.
3. **SSOT Syncing**: Successfully anchored `marksmen-editor` to dynamically read from the `marksmen-cite` local cache.

## Phase 19: Deployment Packaging & Cross-Device Sync [COMPLETE]
- Strategy: Address synchronization of the `references.json` library across isolated devices and package the desktop applications for production release.
- Target: 100% deployment-ready cross-compilation on Windows, macOS, and Linux without local build dependency requirements.

### Tactical Workstreams:
1. **Build Engineering**: Finalize Tauri bundlers to produce `.msi`, `.dmg`, and `.deb` deployment artifacts.
2. **Cloud Synchronization**: Implement an automated `.json` caching loop to a safe remote store or IPFS layer.
3. **Web Importer Extension**: Scaffolding a browser extension to push references directly from scientific journal HTML views into the local daemon.

## Phase 20: Final Closure & Audit Completion [COMPLETE]
- Strategy: Address the remaining enterprise gap audit items (Extensibility, Legal Compliance, Form Generation).
- Target: Achieve 100% parity with Microsoft Word, Adobe Acrobat, and EndNote.

### Tactical Workstreams:
1. **Wasm Extensibility**: Implement `marksmen-plugin` for Pandoc-like WebAssembly AST filters.
2. **Legal Compliance**: Implement destructive redaction (`<redact>`) in Typst and DOCX pipelines.
3. **Memory Optimization**: Complete memory hygiene sweep, pre-allocating Assembly vectors and eliminating redundant AST allocations.

## Phase 21: Enterprise Automation & Intelligence [NEXT]
- Strategy: Resolve the remaining High-Severity workflow gaps (G-H42, G-H43, G-H56, G-H57) targeting Mail Merge, Document Comparison, and AI/Voice Input.
- Target: Achieve parity with Word's advanced automated publishing and accessibility tools.

### Tactical Workstreams:
1. **Mail Merge Engine (G-H43, G-H44)** [COMPLETE]: Implement CSV/JSON data-source ingestion and template variable replacement (`{{field}}`) for batch document/envelope generation.
2. **Document Comparison (G-H42)** [COMPLETE]: Extend `marksmen-diff` to ingest two distinct documents (DOCX or MD) and synthesize a unified Tracked Changes view.
3. **Voice & AI Assistant (G-H56, G-H57)**: Integrate Web Speech API dictation and local LLM context-aware grammar/clarity suggestions.

### Remaining Risks:
- Sourcing a truly private, offline LLM runtime (e.g., ONNX / Llama.cpp) for the AI assistant without compromising the local-first zero-telemetry architecture.
- Mail Merge templating may conflict with raw Markdown AST compilation bounds if not carefully isolated.
