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


1. Item one
2. Item two \
not indented
3. Item three
