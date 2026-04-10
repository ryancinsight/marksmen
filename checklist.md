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
