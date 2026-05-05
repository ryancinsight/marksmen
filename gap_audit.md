# Marksmen ↔ Microsoft Word Gap Audit
> Updated: 2026-05-01 (Rev 5) | Auditor: Antigravity

---

## What Marksmen Has Today

### Backend (Rust crates)

| Crate | Capability | Architecture Note |
|-------|-----------|-------------------|
| `marksmen-core` | Markdown AST parser + config/frontmatter | ✅ Canonical |
| `marksmen-html` / `marksmen-html-read` | HTML ↔ MD roundtrip | ✅ Symmetric |
| `marksmen-docx` / `marksmen-docx-read` | DOCX export + import | ✅ Symmetric |
| `marksmen-odt` / `marksmen-odt-read` | ODT export + import | ✅ Symmetric |
| `marksmen-pdf` / `marksmen-pdf-read` | PDF export + import (Typst-backed) | ✅ Symmetric |
| `marksmen-typst` / `marksmen-typst-read` | Typst source export + import | ✅ Symmetric |
| `marksmen-rich` / `marksmen-rich-read` | RTF export + import | ✅ Symmetric |
| `marksmen-ppt` | PPTX export only | ⚠️ No `marksmen-ppt-read` |
| `marksmen-diff` | Tracked changes diff engine (HTML output) | ✅ Present |
| `marksmen-latex` / `marksmen-latex-read` | LaTeX export + import | ✅ Symmetric |
| `marksmen-mermaid` | Mermaid diagram **rendering** only | ⚠️ No creation UI — see G-ARCH02 |
| `marksmen-marp` / `marksmen-marp-read` | MARP slide deck | ✅ Symmetric |
| `marksmen-xhtml` / `marksmen-xhtml-read` | XHTML roundtrip | ✅ Symmetric |
| `marksmen-xml` / `marksmen-xml-read` | XML roundtrip | ✅ Symmetric |
| `marksmen-render` | Core render pipeline | ✅ Present |
| `marksmen-roundtrip` | Roundtrip fidelity test harness | ✅ Present |

### Frontend (HTML/CSS/JS) — Implemented

| Feature | Status |
|---------|--------|
| Ribbon UI (File, Home, Insert, Layout, Review, View) | ✅ |
| Bold, Italic, Underline, Strikethrough, Super/Subscript | ✅ |
| Pre-emptive format arming + active-state indicators | ✅ |
| Text color, Highlight color | ✅ |
| Font family picker (5 fonts), Font size picker | ✅ |
| Bullet list, Numbered list, Indent/Outdent | ✅ |
| Align Left / Center / Right | ✅ |
| H1–H3, Normal, Quote, Code block styles | ✅ |
| Markdown heading shortcut (`# ` prefix) | ✅ |
| Table: grid picker, custom size, hover toolbar, merge/split, align, delete | ✅ |
| Table cell selection highlight + context menu | ✅ |
| Hyperlink inline dialog | ✅ |
| Image insert (file picker → base64 inline) | ✅ |
| Footnote insert (numbered, appended to body) | ✅ |
| Equation insert (LaTeX prompt, display + inline) | ✅ |
| Horizontal rule insert | ✅ |
| Comment sidebar (bezier arrows, reply threads) | ✅ |
| Document outline sidebar (H1–H6 navigation) | ✅ |
| Tracked changes diff (Set Base → Show Changes) | ✅ |
| Find & Replace (regex, match-case, whole-word) | ✅ |
| Floating selection toolbar (B/I/U/S/Link/Comment/Clear) | ✅ |
| Page count + page break rulers | ✅ |
| Word count, char count, reading time estimate | ✅ |
| Caret position (Ln, Col) in status bar | ✅ |
| Zoom slider (50–200%) | ✅ |
| Print view / Web view layouts | ✅ |
| Focus mode (F11), Dark/Light theme, Spell check toggle | ✅ |
| LocalStorage autosave + session restore | ✅ |
| Markdown paste auto-conversion | ✅ |
| Recent files list | ✅ |
| Settings panel (author, font, theme, page size, autosave interval) | ✅ |
| Custom Styled Shortcut Tooltips | ✅ |
| Read Aloud (Native Text-to-Speech) | ✅ |
| RTL Text Direction & Language Selection | ✅ |
| Table Cell Properties (Padding & Border Dialog) | ✅ |
| Page Watermarks | ✅ |
| Line Numbering in Margin | ✅ |
| Export: DOCX, ODT, PDF, PPTX, RTF, HTML, Typst, Markdown | ✅ |
| Import: MD, HTML, DOCX, ODT, PDF, Typst, RTF | ✅ |
| Native save/save-as (.md), Print via PDF | ✅ |

---

## Gap Registry

### ARCHITECTURE — Crate-level structural violations

| ID | Gap | Severity | Priority |
|----|-----|----------|----------|
| ~~G-ARCH01~~ | ~~**`marksmen-rich` must be split into `marksmen-rich` (export) + `marksmen-rich-read` (import)**~~ | ~~HIGH~~ | ~~Complete~~ |
| ~~G-ARCH02~~ | ~~**`marksmen-mermaid` covers rendering only; diagram *creation* has no UI** — The crate provides no diagram AST builder, live editor, or Insert ribbon entry. Mermaid/Graphviz/draw.io-style creation (code editor + live preview + export as SVG/PNG) must be added at both editor UI and backend levels.~~ | ~~HIGH~~ | ~~Sprint 6~~ |
| ~~G-ARCH03~~ | ~~**`marksmen-ppt` has no `marksmen-ppt-read` counterpart**~~ | ~~MEDIUM~~ | ~~Complete~~ |
| ~~G-ARCH04~~ | ~~**No `marksmen-epub` crate**~~ | ~~MEDIUM~~ | ~~Complete~~ |

---

### CRITICAL — Data integrity / correctness

| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| ~~G-C01~~ | ~~**No multi-format save path** — Ctrl+S always writes `.md` only. DOCX-origin documents lose round-trip fidelity on every save cycle.~~ | ~~File I/O~~ | ~~Backend~~ |
| ~~G-C02~~ | ~~**No disk autosave** — Only localStorage; crash or site-data clear loses all unsaved work silently.~~ | ~~File I/O~~ | ~~Frontend + Backend~~ |
| ~~G-C03~~ | ~~**Image data loss on reload** — Base64 images are not persisted in the Markdown intermediate nor correctly embedded in DOCX/ODT exports.~~ | ~~Images~~ | ~~Frontend + Backend~~ |
| ~~G-C04~~ | ~~**Shallow browser undo stack** — `execCommand` undo does not cover structural operations (table insert, heading shortcut, paste). Custom `MutationObserver` snapshot stack required.~~ | ~~Editor~~ | ~~Frontend~~ |
| ~~G-C05~~ | ~~**No unsaved-changes warning on window close** — Tauri `CloseRequested` event not intercepted to prompt save.~~ | ~~File I/O~~ | ~~Backend~~ |

---

### HIGH — Core Word workflow parity

#### Home Tab Gaps
| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| ~~G-H01~~ | ~~**No paragraph line-spacing control** — No 1.0 / 1.5 / 2.0 / Custom spacing dropdown in ribbon.~~ | ~~Paragraph~~ | ~~Frontend~~ |
| ~~G-H02~~ | ~~**No paragraph space-before / space-after** in ribbon.~~ | ~~Paragraph~~ | ~~Frontend~~ |
| ~~G-H07~~ | ~~**No styles gallery** — Only 6 fixed text buttons; no scrollable Quick Styles gallery with visual thumbnails, named styles, or style editing.~~ | ~~Styles~~ | ~~Frontend~~ |
| ~~G-H16~~ | ~~**No multi-level list dropdown** — Single depth only; no outline numbering (1.1, 1.1.1, A.1).~~ | ~~Lists~~ | ~~Frontend~~ |
| ~~G-H17~~ | ~~**No list style picker** — No bullet shape or number format selection dropdown.~~ | ~~Lists~~ | ~~Frontend~~ |
| ~~G-H23~~ | ~~**No justify alignment (Ctrl+J)** — Full text justification missing.~~ | ~~Paragraph~~ | ~~Frontend~~ |
| ~~G-H25~~ | ~~**Font picker too narrow** — 5 hardcoded fonts; no system font enumeration.~~ | ~~Font~~ | ~~Frontend~~ |
| ~~G-H26~~ | ~~**No type-to-filter font combobox** — `<select>` not a searchable combobox.~~ | ~~Font~~ | ~~Frontend~~ |
| ~~G-H27~~ | ~~**No font grow / shrink buttons** — A+ / A- increment missing.~~ | ~~Font~~ | ~~Frontend~~ |
| ~~G-H45~~ | ~~**No Format Painter** — Cannot copy all formatting from selected text and apply it to another range with one click. Word's most-used productivity feature after Bold/Italic.~~ | ~~Clipboard~~ | ~~Frontend~~ |
| ~~G-H46~~ | ~~**No Change Case (Aa) button** — Cannot cycle UPPERCASE / lowercase / Title Case / Sentence case / tOGGLE cAsE on selection.~~ | ~~Font~~ | ~~Frontend~~ |
| ~~G-H47~~ | ~~**No underline style dropdown** — Underline button applies only solid single underline; no access to double, dotted, dashed, thick, word-only, or colored underline variants.~~ | ~~Font~~ | ~~Frontend~~ |
| ~~G-H48~~ | ~~**No Text Effects & Typography** — No outline, glow, reflection, shadow, or ligature/stylistic set controls on selected text.~~ | ~~Font~~ | ~~Frontend~~ |
| ~~G-H49~~ | ~~**No Clear Formatting in ribbon** — Clear All Formatting (eraser icon) missing from Font group in Home ribbon; currently only accessible via floating selection toolbar.~~ | ~~Font~~ | ~~Frontend~~ |
| ~~G-H50~~ | ~~**No Show/Hide ¶ (formatting marks)** — Cannot reveal non-printing characters (spaces, tabs, paragraph marks, section breaks, page breaks) for debugging layout.~~ | ~~View~~ | ~~Frontend~~ |
| ~~G-H51~~ | ~~**No Sort button in Paragraph group** — Cannot alphabetically or numerically sort a selected list or table column from the ribbon.~~ | ~~Paragraph~~ | ~~Frontend~~ |
| ~~G-H52~~ | ~~**No Shading dropdown in Paragraph group** — No paragraph background color bucket (distinct from text highlight; applies to entire paragraph block).~~ | ~~Paragraph~~ | ~~Frontend~~ |
| ~~G-H53~~ | ~~**No Borders dropdown in Paragraph group** — No quick-access border style gallery (All Borders, Outside Borders, Bottom Border, etc.) for paragraph and table.~~ | ~~Paragraph~~ | ~~Frontend~~ |
| ~~G-H54~~ | ~~**No Editing ribbon group** — Find, Replace, and Select are keyboard-only; no visible Editing group with Find / Replace / Select dropdown in the ribbon.~~ | ~~Editing~~ | ~~Frontend~~ |
| ~~G-H55~~ | ~~**No Select submenu** — No Select All / Select Objects / Select All Text with Similar Formatting options.~~ | ~~Editing~~ | ~~Frontend~~ |
| G-H56 | **No Dictate button** — No voice input via Web Speech API or OS speech recognition. | Voice | Frontend |
| G-H57 | **No AI writing assistant (Editor)** — No grammar, clarity, conciseness, or inclusiveness suggestion panel. | AI | Frontend + Backend |

#### Insert Tab Gaps
| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| ~~G-H08~~ | ~~**No Table of Contents generation** — No auto-TOC from headings.~~ | ~~Insert~~ | ~~Frontend~~ |
| ~~G-H13~~ | ~~**No image resize handles** — Inserted images are fixed max-width.~~ | ~~Images~~ | ~~Frontend~~ |
| ~~G-H14~~ | ~~**No image text wrapping modes** — Inline/square/tight/float-left/float-right missing.~~ | ~~Images~~ | ~~Frontend~~ |
| ~~G-H15~~ | ~~**No image caption / figure numbering** — No `<figure>`/`<figcaption>` workflow or alt-text dialog.~~ | ~~Images~~ | ~~Frontend~~ |
| ~~G-H28~~ | ~~**No cover page / pre-built page templates**~~ — Insert → Cover Page opens gallery with 4 templates (Classic, Modern, Minimal, Executive); selected template is prepended to editor HTML. | ~~Insert~~ | ~~Frontend~~ |
| ~~G-H29~~ | ~~**No symbol / special character picker** — No Unicode character map dialog.~~ | ~~Insert~~ | ~~Frontend~~ |
| G-H30 | **No screenshot / screen clipping capture** — Word's Insert → Screenshot workflow. | Insert | Frontend + Backend |

#### Layout Tab Gaps
| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| ~~G-H03~~ | ~~**No page margins UI** — Margins fixed in CSS; no dialog.~~ | ~~Page Layout~~ | ~~Frontend~~ |
| ~~G-H04~~ | ~~**No page orientation toggle** — Portrait/Landscape missing.~~ | ~~Page Layout~~ | ~~Frontend + Backend~~ |
| ~~G-H05~~ | ~~**No page size selector** — US Letter only; no A4, Legal, Custom.~~ | ~~Page Layout~~ | ~~Frontend + Backend~~ |
| ~~G-H06~~ | ~~**No columns layout** — Single column only.~~ | ~~Page Layout~~ | ~~Frontend + Backend~~ |
| ~~G-H10~~ | ~~**No header/footer zones** — No section header/footer with page number, date, document title fields.~~ | ~~Page Layout~~ | ~~Frontend + Backend~~ |
| ~~G-H11~~ | ~~**No page number fields** — No auto-incrementing page number insert.~~ | ~~Page Layout~~ | ~~Frontend + Backend~~ |
| ~~G-H12~~ | ~~**No section breaks** — No next-page, continuous, even/odd page breaks.~~ | ~~Page Layout~~ | ~~Frontend~~ |
| ~~G-H31~~ | ~~**No hyphenation control** — No auto-hyphenation for justified text.~~ | ~~Paragraph~~ | ~~Frontend + Backend~~ |
| ~~G-H32~~ | ~~**No line numbers** — Cannot show line numbers in margin (used in legal, academic contexts).~~ | ~~Page Layout~~ | ~~Frontend + Backend~~ |

#### References Tab Gaps (entire tab missing)
| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| ~~G-H33~~ | ~~**No References ribbon tab**~~ — References tab added covering TOC, Footnotes, Citations, Captions, Index, Bookmarks. | ~~References~~ | ~~Frontend~~ |
| ~~G-H34~~ | ~~**No citation / bibliography manager**~~ — APA source manager added; Insert Citation and Generate Bibliography implemented. | ~~Academic~~ | ~~Frontend + Backend~~ |
| ~~G-H35~~ | ~~**No figure/table caption auto-numbering**~~ — Caption button inserts auto-numbered Figure/Table labels. | ~~Academic~~ | ~~Frontend~~ |
| ~~G-H36~~ | ~~**No cross-reference insert**~~ — Cross-ref picker lists headings, figures, tables, footnotes, bookmarks by type. | ~~Academic~~ | ~~Frontend~~ |
| ~~G-H37~~ | ~~**No index generation**~~ — Mark Entry + Generate Index workflow added. | ~~Academic~~ | ~~Frontend + Backend~~ |
| ~~G-H38~~ | ~~**No table of figures / table of tables**~~ — Table of Figures and Table of Tables auto-generated from figcaption elements. | ~~Academic~~ | ~~Frontend~~ |
| G-H39 | **No table of authorities** — Legal citation table missing. | Legal | Frontend + Backend |

#### Review Tab Gaps
| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| ~~G-H18~~ | ~~**No tracked changes accept/reject UI**~~ — `generate_diff` renders a diff; per-change accept/reject now implemented. | ~~Review~~ | ~~Frontend + Backend~~ |
| ~~G-H19~~ | ~~**No comment resolution**~~ — Comments now have a "Resolved" state with per-card resolve/re-open toggle and Resolve All. | ~~Review~~ | ~~Frontend~~ |
| ~~G-H40~~ | ~~**No author-attributed tracked changes**~~ — Changes now tagged with `data-author` and `data-date`. | ~~Review~~ | ~~Frontend + Backend~~ |
| ~~G-H41~~ | ~~**No change tracking on by default option**~~ — Track Changes toggle button added to Review ribbon (Ctrl+Shift+E). | ~~Review~~ | ~~Frontend~~ |
| G-H42 | **No compare documents** — Cannot load two separate `.md`/`.docx` files and compare them. | Review | Backend |

#### Mailings Tab (entire tab missing)
| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| G-H43 | **No mail merge** — No merge fields, data source connection, or merge preview. | Mailings | Backend |
| G-H44 | **No label / envelope printing** — Standard business workflow. | Mailings | Frontend + Backend |

---

### MEDIUM — Productivity / UX quality

| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| ~~G-M01~~ | ~~**No keyboard shortcut reference panel**~~ — `?` key opens `shortcuts-scrim` modal with categorised shortcut grid. | ~~UX~~ | ~~Frontend~~ |
| ~~G-M02~~ | ~~**No smart quotes / autocorrect**~~ — Curly quotes, em-dash (`--`), ellipsis (`...`), `(c)` → ©, `(r)` → ®, `(tm)` → ™ autocorrect now active on every keyup. | ~~Editing~~ | ~~Frontend~~ |
| G-M03 | **No paste special** — No "Paste as Plain Text" / "Keep Formatting" / "Merge Formatting" option. | Clipboard | Frontend |
| ~~G-M04~~ | ~~**No ruler (horizontal/vertical)**~~ — Horizontal ruler with 96px/inch tick marks, inch number labels (1–8), draggable left/right margin handles, and first-line indent handle. | ~~Layout~~ | ~~Frontend~~ |
| ~~G-M05~~ | ~~**No configurable tab stops**~~ — Click on ruler marks area to place tab-stop markers; click existing marker to remove. | ~~Paragraph~~ | ~~Frontend~~ |
| G-M06 | **Equation editor prompt-only** — No visual equation builder, symbol palette, or MathML preview. | Equations | Frontend |
| ~~G-M07~~ | ~~**No border/shading dialog** — Table cell and paragraph borders fixed; no per-side control, color, or style.~~ | ~~Formatting~~ | ~~Frontend + Backend~~ |
| ~~G-M08~~ | ~~**No RTL text direction** — No Arabic/Hebrew/Urdu writing direction support.~~ | ~~I18N~~ | ~~Frontend + Backend~~ |
| ~~G-M09~~ | ~~**No read-aloud / text-to-speech** — No accessibility speech synthesis.~~ | ~~Accessibility~~ | ~~Frontend~~ |
| ~~G-M10~~ | ~~**No per-paragraph language tag** — Spell check uses browser global locale only.~~ | ~~I18N~~ | ~~Frontend~~ |
| ~~G-M11~~ | ~~**No bookmark / named anchor**~~ — Bookmark button in References tab inserts `<a name>` anchors; cross-ref picker lists bookmarks as targets. | ~~Insert~~ | ~~Frontend~~ |
| ~~G-M12~~ | ~~**Mermaid diagram creation absent** — `marksmen-mermaid` renders only; no Insert → Diagram with code editor + live preview.~~ | ~~Insert~~ | ~~Frontend + Backend~~ |
| G-M13 | **No chart insert** — No bar/line/pie chart builder (Word embeds Excel chart). | Insert | Frontend + Backend |
| ~~G-M14~~ | ~~**No shape / drawing tools** — No rectangle, circle, arrow, callout drawing on canvas.~~ | ~~Insert~~ | ~~Frontend~~ |
| G-M15 | **No SmartArt builder** — No structured diagram (org chart, process, cycle, pyramid) editor. | Insert | Frontend |
| ~~G-M16~~ | ~~**No watermark** — Cannot add DRAFT / CONFIDENTIAL page background.~~ | ~~Page Layout~~ | ~~Frontend~~ |
| ~~G-M17~~ | ~~**No drop cap**~~ — Layout tab → Drop Cap toggle applies `data-drop-cap` attribute; CSS `::first-letter` float with accent color + 3.8em size. | ~~Typography~~ | ~~Frontend~~ |
| G-M18 | **No text boxes / floating frames** — All content is in-flow; no absolutely-positioned text frame. | Layout | Frontend + Backend |
| ~~G-M19~~ | ~~**Footnote print placement**~~ — `@media print` applies `position:fixed;bottom:0` to `.footnote-def` so footnotes appear at page bottom during print. | ~~Footnotes~~ | ~~Frontend + Backend~~ |
| ~~G-M20~~ | ~~**No endnote support**~~ — Endnote button in References tab inserts numbered `[N]` marker and appends endnote body at document end. | ~~Footnotes~~ | ~~Frontend + Backend~~ |
| G-M21 | **No table header row repeat on page break** — `<thead>` not repeated across breaks in DOCX/PDF. | Tables | Backend |
| ~~G-M22~~ | ~~**No table styles gallery** — Only plain table; no predefined striped/bordered/header-colored styles.~~ | ~~Tables~~ | ~~Frontend~~ |
| ~~G-M23~~ | ~~**No cell padding / spacing control** — Fixed CSS table cell padding; no per-table override UI.~~ | ~~Tables~~ | ~~Frontend~~ |
| ~~G-M24~~ | ~~**No F3 / Shift+F3 Find navigation**~~ — F3 opens find bar; F3 when open = next match; Shift+F3 = previous match. | ~~Find~~ | ~~Frontend~~ |
| ~~G-M25~~ | ~~**Export missing XLSX, EPUB, OPML**~~ — EPUB 3 export implemented; multi-chapter OPF/NCX. | ~~Export~~ | ~~Backend~~ |
| ~~G-M26~~ | ~~**No DOCX template support**~~ — `template_path` config field; ZIP-swap injects `word/document.xml` into `.dotx` skeleton. | ~~Export~~ | ~~Backend~~ |
| ~~G-M27~~ | ~~**No PDF/A compliance mode**~~ — `pdf_standard: pdf-a` frontmatter activates `PdfStandards::new(&[PdfStandard::A_1b])` in Typst exporter. | ~~Export~~ | ~~Backend~~ |
| ~~G-M28~~ | ~~**No version history**~~ — Named snapshots stored in `localStorage` (up to 50); 5-min auto-snapshot; manual via File → Version History; restore/rename/delete per entry. | ~~File~~ | ~~Frontend~~ |
| G-M29 | **No real-time collaboration** — Single-user only; no WebSocket/CRDT co-editing. | Collaboration | Backend |
| ~~G-M30~~ | ~~**No word/character count in selection**~~ — `selectionchange` drives `sbar-word-count` with `(N selected)` suffix; collapses cleanly. | ~~Status~~ | ~~Frontend~~ |
| ~~G-M31~~ | ~~**No document properties dialog**~~ — Modal with title/author/subject/keywords/date fields; persisted in localStorage; drives `window.docProps`; accessible via Ctrl+Shift+D. | ~~File~~ | ~~Frontend~~ |
| ~~G-M32~~ | ~~**No print preview**~~ — Print Preview modal shows estimated page count, word count, orientation and color-mode selectors before calling `printDocument()`; Ctrl+P opens preview. | ~~Print~~ | ~~Frontend~~ |
| ~~G-M33~~ | ~~**No reading mode / protected view**~~ — Read mode (View tab → Read Mode) sets `contentEditable=false`, hides ribbon; pre-existing implementation confirmed. | ~~View~~ | ~~Frontend~~ |
| ~~G-M34~~ | ~~**No word frequency / readability stats** — No Flesch-Kincaid, grade level, or sentence stats.~~ | ~~Stats~~ | ~~Frontend~~ |
| G-M35 | **No macro / script recording** — No automation for repetitive task sequences. | Advanced | Frontend |
| ~~G-M36~~ | ~~**No form fields** — No check boxes, dropdown lists, date pickers for fillable forms.~~ | ~~Forms~~ | ~~Frontend + Backend~~ |

---

### LOW — Polish / edge cases

| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| ~~G-L01~~ | ~~Styled shortcut pills missing from ribbon tooltips (only raw `title=` attribute).~~ | ~~UX~~ | ~~Frontend~~ |
| ~~G-L02~~ | ~~`pastePlain` fails in Tauri — `navigator.clipboard.readText` requires explicit permission; use `tauri-plugin-clipboard-manager`.~~ | ~~Clipboard~~ | ~~Backend~~ |
| ~~G-L03~~ | ~~No `aria-live` on status bar updates (word count, sync status, caret).~~ | ~~Accessibility~~ | ~~Frontend~~ |
| ~~G-L04~~ | ~~No dark-mode-aware print CSS — print uses light colors regardless of active theme.~~ | ~~Print~~ | ~~CSS~~ |
| ~~G-L05~~ | ~~Heading-flash animation fires once; heading remains unstyled when cursor stays on it.~~ | ~~Editing~~ | ~~CSS~~ |
| ~~G-L06~~ | ~~Table grid picker capped at 8×10; no grow-on-hover beyond boundary.~~ | ~~Tables~~ | ~~Frontend~~ |
| ~~G-L07~~ | ~~No drag-and-drop file open (drop DOCX onto window).~~ | ~~File I/O~~ | ~~Frontend + Backend~~ |
| ~~G-L08~~ | ~~Font size picker does not sync when font size applied via span-wrap toolbar action.~~ | ~~Font~~ | ~~Frontend~~ |
| ~~G-L09~~ | ~~No "select all in table" keyboard shortcut.~~ | ~~Tables~~ | ~~Frontend~~ |
| ~~G-L10~~ | ~~Outline sidebar does not highlight current heading as user scrolls.~~ | ~~Sidebar~~ | ~~Frontend~~ |

---

### DESIGN TAB — Entire Word ribbon tab missing from Marksmen

> Word's **Design** tab covers document-wide theme and visual identity. Marksmen has no equivalent.

| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| G-D01 | **No Design ribbon tab** — No document themes, style sets, colors, or fonts at the document level. | Design | Frontend |
| G-D02 | **No document themes** — Word ships with 20+ named themes (Office, Facet, Ion, Slice, etc.) each applying a coordinated color + font + effect set. Marksmen has only Dark/Light mode. | Design | Frontend |
| ~~G-D03~~ | ~~**No style sets** — Document-wide paragraph formatting presets (Basic / Casual / Formal / Word 2003 / etc.) that redefine all heading and body styles at once.~~ | ~~Design~~ | ~~Frontend~~ |
| ~~G-D04~~ | ~~**No theme color palette picker** — Change the 10-color palette used by all text/highlight/border colors throughout the document.~~ | ~~Design~~ | ~~Frontend~~ |
| ~~G-D05~~ | ~~**No theme font pair picker** — Change the heading/body font pair for the whole document simultaneously (e.g., Calibri/Calibri Light → Times New Roman/Arial).~~ | ~~Design~~ | ~~Frontend~~ |
| G-D06 | **No document-wide paragraph spacing preset** — Design → Paragraph Spacing applies a named spacing profile (Compact / Tight / Open / Double / etc.) to every paragraph block. | Design | Frontend |
| G-D07 | **No page color fill** — Design → Page Color sets a background fill (solid, gradient, texture, image) for the entire document canvas; exported to DOCX `<w:background>`. | Design | Frontend + Backend |
| ~~G-D08~~ | ~~**No page borders**~~ — Design → Page Borders opens full dialog (style: solid/dashed/dotted/double/groove/ridge; color picker; width 1–12px slider; scope: whole doc / first page only; live preview). | ~~Design~~ | ~~Frontend + Backend~~ |
| G-D09 | **No "Set as Default" document style** — Cannot save current theme + style set as the new blank-document default. | Design | Frontend |

---

### VIEW TAB — Partial; significant groups missing

> Marksmen View panel has Zoom presets, Theme toggle, Focus Mode, Spell Check, and Print. The following Word View groups are absent.

| ID | Gap | Domain | Layer |
|----|-----|--------|-------|
| ~~G-V01~~ | ~~**No view mode switching**~~ — Print / Read / Web / Draft / Outline modes implemented via `setViewMode()`; body class strategy. | ~~View~~ | ~~Frontend~~ |
| G-V02 | **No Immersive Reader** — Word's accessible full-screen column reading mode with line focus, syllable separation, and text spacing controls. | View | Frontend |
| ~~G-V03~~ | ~~**No Gridlines overlay**~~ — `btn-gridlines` toggle adds `show-gridlines` class; CSS injects a dot/line grid via `background-image` linear-gradient. | ~~View~~ | ~~Frontend~~ |
| ~~G-V04~~ | ~~**No Navigation Pane toggle**~~ — `btn-nav-pane` opens sidebar to the outline tab; `btn-toggle` on collapse/expand. | ~~View~~ | ~~Frontend~~ |
| ~~G-V05~~ | ~~**No Zoom dialog**~~ — `btn-zoom-dialog` opens overlay with 75/100/125/150/200% presets and free-input custom % field. | ~~View~~ | ~~Frontend~~ |
| G-V06 | **No window split** — View → Split divides the window into two independent scroll panes of the same document (useful for comparing sections). | View | Frontend |
| G-V07 | **No side-by-side window** — View → View Side by Side opens two documents in synchronized scroll panes. | View | Frontend |
| ~~G-V08~~ | ~~**No Outline view**~~ — View → Outline mode shows floating panel listing all headings with indent hierarchy; Promote/Demote buttons change heading level (H1↔H6); clicking scrolls to heading. | ~~View~~ | ~~Frontend~~ |
| ~~G-V09~~ | ~~**No Draft view**~~ — `view-draft` body class hides images and page-break rulers; monospace font; no page shadow. | ~~View~~ | ~~Frontend~~ |
| G-V10 | **No Macros panel in View** — View → Macros opens the macro recorder and run dialog. | Macros | Frontend |
| G-V11 | **No page thumbnail strip** — Word's Navigation Pane includes a Pages tab showing miniature page thumbnails for click-to-jump navigation. | View | Frontend |

---

## Summary Counts

| Severity | Count |
|----------|-------|
| ARCHITECTURE | 4 |
| CRITICAL | 5 |
| HIGH | 49 |
| DESIGN TAB | 9 |
| VIEW TAB | 11 |
| MEDIUM | 36 |
| LOW | 10 |
| **Total** | **124** |

## Priority Resolution Order (sprint triage)
1. ~~**ARCH01** — Split `marksmen-rich` → `marksmen-rich` + `marksmen-rich-read`~~ (Completed)
2. **C01–C05** — Crash safety / data loss prevention
3. **H01–H06, H10–H12** — Page layout (paragraph spacing, margins, header/footer, page numbers, breaks)
4. **H45–H49** — Home tab critical UX: Format Painter, Change Case, Underline styles, Text Effects, Clear Formatting in ribbon
5. **D01–D09** — Design tab (themes, style sets, page color, page borders)
6. **H07, H16–H17, H50–H53** — Styles gallery, multi-level lists, Show/Hide ¶, Sort, Shading, Borders dropdown
7. **H13–H15** — Image resize, wrapping, captions
8. **H18–H19, H40–H42** — Tracked changes accept/reject, comment resolution, author attribution
9. **H33–H38** — References tab (TOC, citations, captions, index)
10. **H23, H27, H54–H55, ARCH02** — Justify, font grow/shrink, Editing group, Select submenu, Mermaid creation UI
11. **V01–V11** — View tab (mode switching, gridlines, zoom dialog, split, outline view)
12. **M01–M07** — Autocorrect, shortcut panel, rulers, equation palette
13. **H56, M35** — Dictate, Macro recording
14. **M25** — EPUB export, PPT read, remaining export crates
