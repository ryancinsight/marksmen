












# Output Capabilities & Typst Extensibility



## 1. Document Headers & Configuration


Headers, footers, and page numbers are intrinsically supported via YAML frontmatter blocks. For example, this document has a custom footer string.


## 2. Advanced Typography & Tables


Because the intermediate format is Typst, typography is mathematically precise. Let's look at a standard Markdown table:




### 2.1 CSS & HTML Handling


Typst is an analytical typesetting engine, . It cannot execute CSS or complex DOM layouts. However, the translator enforces semantic mapping for structural inline HTML subsets:


- :  via 

- : HO via 

- : E = mc via 

- : This text is colored via 

- : Forced HTML break  is translated correctly.




## 3. Mathematical Typesetting


True native math (without KaTeX SVG hacks):

$ 

 $

Inline equations like $ $ blend seamlessly with the baseline.


## 4. Linked SVG Figures


Instead of relying on web-based JS to render graphics, Typst handles  vector embeddings natively without rasterization:


Architecture Diagram


## 5. Native Diagram Rendering ()









## Appendix: Mathematical Justifications


Because  leverages Typst, we can seamlessly write $$ in the middle of text, and be confident that it renders beautifully on a completely separated page.
