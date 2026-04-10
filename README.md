# marksmen

`marksmen` is a Rust workspace for converting Markdown into editable and publishable document formats while preserving as much structural information as possible for roundtrip extraction.

Current targets include:
- `PDF`
- `DOCX`
- `ODT`
- `HTML`

The workspace also includes companion reader crates for extracting Markdown-like content back from those formats, plus roundtrip tests and example binaries.

## What It Does

- Parses Markdown with frontmatter into a shared event stream.
- Converts the same source into multiple output formats.
- Supports inline and display math.
- Renders Mermaid-style diagrams through a native Rust layout pipeline.
- Measures roundtrip similarity by re-extracting generated documents back into Markdown-like text.

## Workspace Layout

- `crates/marksmen`
  The CLI entry point.
- `crates/marksmen-core`
  Shared config, frontmatter parsing, and Markdown parsing.
- `crates/marksmen-pdf`
  Markdown to PDF via Typst.
- `crates/marksmen-docx`
  Markdown to DOCX.
- `crates/marksmen-odt`
  Markdown to ODT.
- `crates/marksmen-html`
  Markdown to HTML.
- `crates/marksmen-mermaid`
  Native Mermaid parsing, layout, and rendering support.
- `crates/marksmen-docx-read`
  DOCX back to Markdown-like text.
- `crates/marksmen-odt-read`
  ODT back to Markdown-like text.
- `crates/marksmen-pdf-read`
  PDF text extraction for roundtrip evaluation.
- `crates/marksmen-html-read`
  HTML back to Markdown-like text.
- `crates/marksmen-roundtrip`
  Similarity tests and roundtrip assessment harnesses.
- `examples`
  Small runnable examples for creation, conversion, roundtrips, and symmetry checks.

## CLI Usage

Build and run:

```powershell
cargo run -p marksmen --target x86_64-pc-windows-msvc -- input.md
```

Generate a specific output:

```powershell
cargo run -p marksmen --target x86_64-pc-windows-msvc -- input.md -o output.docx
cargo run -p marksmen --target x86_64-pc-windows-msvc -- input.md -o output.odt
cargo run -p marksmen --target x86_64-pc-windows-msvc -- input.md -o output.pdf
```

Useful options:

```text
--no-math
--as-typst
--watch
--page-width
--page-height
--margin
```

## Examples

Run the example binaries from the `examples` workspace crate:

```powershell
cargo run -p marksmen-examples --bin creation --target x86_64-pc-windows-msvc
cargo run -p marksmen-examples --bin conversions --target x86_64-pc-windows-msvc
cargo run -p marksmen-examples --bin roundtrips --target x86_64-pc-windows-msvc
cargo run -p marksmen-examples --bin symmetry_assessments --target x86_64-pc-windows-msvc
```

## Roundtrip Testing

The workspace includes readers and similarity checks for generated outputs.

Examples:

```powershell
cargo test -p marksmen-html-read --target x86_64-pc-windows-msvc
cargo test -p marksmen-roundtrip test_html_roundtrip_similarity --target x86_64-pc-windows-msvc -- --nocapture
```

## Windows Toolchain Note

On this machine, the most reliable target is:

```text
x86_64-pc-windows-msvc
```

The workspace can also be used with `x86_64-pc-windows-gnu`, but that requires Cargo to resolve a compatible MinGW GCC toolchain. If GNU builds fail on crates like `psm` or `stacker`, prefer the MSVC target unless the GNU toolchain is explicitly configured.

## Status

This repository is under active development. The conversion and roundtrip paths are functional, but formatting fidelity and semantic symmetry are still being improved, especially across complex diagrams, tables, and rich document layouts.
