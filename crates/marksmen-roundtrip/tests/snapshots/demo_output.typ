#set page(width: 210mm, height: 297mm, margin: (top: 30mm, right: 25mm, bottom: 30mm, left: 25mm),
)
#set text(font: "Arial", size: 11pt)
#show raw: set text(font: ("Consolas", "Courier New", "monospace"), size: 10pt)
#show raw.where(block: false): it => highlight(fill: luma(245), extent: 1.5pt)[#it]
#show raw.where(block: true): it => block(
  fill: luma(246),
  inset: (x: 10pt, y: 8pt),
  radius: 3pt,
  width: 100%,
  breakable: false,
)[#it]
#set raw(theme: none)
#set heading(numbering: none)
#show heading: set text(weight: "regular", font: "Arial")
#set par(justify: true, spacing: 1.2em)
#set enum(number-align: start, indent: 1em)
#set list(indent: 1em)


= Output Capabilities & Typst Extensibility

== 1. Document Headers & Configuration

Headers, footers, and page numbers are intrinsically supported via YAML frontmatter blocks. For example, this document has a custom footer string.

== 2. Advanced Typography & Tables

Because the intermediate format is Typst, typography is mathematically precise. Let's look at a standard Markdown table:

#align(center)[
#table(
  columns: (1fr, 1fr, 1fr, 1fr),
  inset: 3pt,
  align: (left, center, right, center),
  [ *Subsystem* ],
  [ *Underlying Tech* ],
  [ *Role* ],
  [ *Status* ],
  [ #strong[Parser] ],
  [ `pulldown-cmark` ],
  [ Emits syntax events ],
  [ ✅ ],
  [ #strong[Translator] ],
  [ `marksmen-core` ],
  [ Maps AST to Typst ],
  [ ✅ ],
  [ #strong[Compiler] ],
  [ `typst` ],
  [ Typesets elements ],
  [ ✅ ],
  [ #strong[Exporter] ],
  [ `typst-pdf` ],
  [ Emits binary format ],
  [ ✅ ],
)
]

=== 2.1 CSS & HTML Handling

Typst is an analytical typesetting engine, #strong[not a web browser]. It cannot execute CSS or complex DOM layouts. However, the translator enforces semantic mapping for structural inline HTML subsets:

- #strong[Underline]: #underline[This text is underlined] via `<u>`
- #strong[Subscripts]: H#sub[2]O via `<sub>`
- #strong[Superscripts]: E = mc#super[2] via `<sup>`
- #strong[Colors]: This text is colored via `<span style="color: ...">`
- #strong[Line breaks]: Forced HTML break #linebreak() is translated correctly.

#emph[Note: Arbitrary `<div style="padding: 2px">` blocks are ignored, preserving architectural invariants against empirical DOM approximations.]

== 3. Mathematical Typesetting

True native math (without KaTeX SVG hacks):

$ 
bold(J) =  nabla   times  bold(H) =  sigma  bold(E) + frac( diff  bold(D),  diff   t)
 $


Inline equations like $ lim _(x  arrow.r   infinity ) f(x)$ blend seamlessly with the baseline.

== 4. Linked SVG Figures

Instead of relying on web-based JS to render graphics, Typst handles `SVG` vector embeddings natively without rasterization:


#image("./diagram1.svg")
Architecture Diagram

== 5. Native Diagram Rendering (`marksmen-mermaid`)

#block(inset: (left: 1em), stroke: (left: 2pt + gray))[
#strong[Pure-Rust Mermaid Translation:]
Executing standard `mermaid.js` typically requires evaluating an entire web runtime (V8/Browser) with empirical DOM measurement APIs, which violates our offline-first pure-Rust correctness guarantees.
#emph[Instead, `marksmen` relies on a mathematically-verified `marksmen-mermaid` layout engine implementing the Sugiyama Framework. It parses this raw Mermaid block offline and renders it deterministically into Typst spatial vectors!]
]

#align(center)[
  #scale(x: 100%, y: 100%, reflow: true)[
    #box(
      width: 396pt,
      height: 510pt,
      clip: false
    )[
  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1.5pt + rgb("#555555"), closed: false, (78pt, 50pt), (81.33480989476226pt, 100.02214842143391pt))]
  #place(dx: 0pt, dy: 0pt)[#path(fill: rgb("#555555"), stroke: 0.6pt + rgb("#555555"), closed: true, (82pt, 110pt), (77.84256184226413pt, 100.25496495826712pt), (84.82705794726039pt, 99.7893318846007pt))]
  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1.5pt + rgb("#555555"), closed: false, (82pt, 140pt), (82pt, 190pt))]
  #place(dx: 0pt, dy: 0pt)[#path(fill: rgb("#555555"), stroke: 0.6pt + rgb("#555555"), closed: true, (82pt, 200pt), (78.5pt, 190pt), (85.5pt, 190pt))]
  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1.5pt + rgb("#555555"), closed: false, (82pt, 230pt), (82pt, 260pt), (70pt, 260pt), (70pt, 280pt))]
  #place(dx: 0pt, dy: 0pt)[#path(fill: rgb("#555555"), stroke: 0.6pt + rgb("#555555"), closed: true, (70pt, 290pt), (66.5pt, 280pt), (73.5pt, 280pt))]
  #place(dx: 0pt, dy: 0pt)[#path(fill: none, stroke: 1.5pt + rgb("#555555"), closed: false, (82pt, 230pt), (82pt, 260pt), (218pt, 260pt), (218pt, 280pt))]
  #place(dx: 0pt, dy: 0pt)[#path(fill: rgb("#555555"), stroke: 0.6pt + rgb("#555555"), closed: true, (218pt, 290pt), (214.5pt, 280pt), (221.5pt, 280pt))]
  #place(dx: 20pt, dy: 200pt)[#rect(width: 124pt, height: 30pt, radius: 0pt, fill: white, stroke: 1.5pt + rgb("#333333"))[#align(center + horizon)[#text(fill: rgb("#222222"))[Typst Vectors]]]]
  #place(dx: 20pt, dy: 110pt)[#rect(width: 124pt, height: 30pt, radius: 0pt, fill: white, stroke: 1.5pt + rgb("#333333"))[#align(center + horizon)[#text(fill: rgb("#222222"))[Marksmen Core]]]]
  #place(dx: 20pt, dy: 20pt)[#rect(width: 116pt, height: 30pt, radius: 0pt, fill: white, stroke: 1.5pt + rgb("#333333"))[#align(center + horizon)[#text(fill: rgb("#222222"))[Markdown AST]]]]
  #place(dx: 160pt, dy: 290pt)[#rect(width: 116pt, height: 30pt, radius: 10pt, fill: white, stroke: 1.5pt + rgb("#333333"))[#align(center + horizon)[#text(fill: rgb("#222222"))[HTML5 Output]]]]
  #place(dx: 20pt, dy: 290pt)[#rect(width: 100pt, height: 30pt, radius: 10pt, fill: white, stroke: 1.5pt + rgb("#333333"))[#align(center + horizon)[#text(fill: rgb("#222222"))[PDF Output]]]]
    ]
  ]
]


#pagebreak()


== Appendix: Mathematical Justifications

Because `marksmen` leverages Typst, we can seamlessly write $e^(i  p i) + 1 = 0$ in the middle of text, and be confident that it renders beautifully on a completely separated page.
