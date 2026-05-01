# Marksmen Strategy Backlog

## Phase 18: Full Stack Citation Manager Deployment [COMPLETE]
- Strategy: Complete transition of `marksmen-cite` from an MVP to a production-ready, feature-complete citation database matching EndNote/Mendeley.
- Target: Lossless web fetching, legacy DB parsers, zero-cost architecture.

### Tactical Workstreams:
1. **Automated Crossref Resolution**: Fully deployed deep PDF geometry extraction and `reqwest` DOI fetching for zero-input metadata hydration.
2. **Legacy Import Strategy**: Built and deployed `.ris` and `.bib` native format parsers to safely onboard legacy library users.
3. **SSOT Syncing**: Successfully anchored `marksmen-editor` to dynamically read from the `marksmen-cite` local cache.

## Phase 19: Deployment Packaging & Cross-Device Sync [NEXT]
- Strategy: Address synchronization of the `references.json` library across isolated devices and package the desktop applications for production release.
- Target: 100% deployment-ready cross-compilation on Windows, macOS, and Linux without local build dependency requirements.

### Tactical Workstreams:
1. **Build Engineering**: Finalize Tauri bundlers to produce `.msi`, `.dmg`, and `.deb` deployment artifacts.
2. **Cloud Synchronization**: Implement an automated `.json` caching loop to a safe remote store or IPFS layer.
3. **Web Importer Extension**: Scaffolding a browser extension to push references directly from scientific journal HTML views into the local daemon.
