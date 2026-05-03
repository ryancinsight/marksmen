# Output Capabilities & Typst Extensibility


## 1. Document Headers & Configuration


Headers, footers, and page numbers are intrinsically supported via YAML frontmatter blocks. For example, this document has a custom footer string.


## 2. Advanced Typography & Tables


Because the intermediate format is Typst, typography is mathematically precise. Let's look at a standard Markdown table:

\begin{longtable}{lcrc}
\toprule
Subsystem & Underlying Tech & Role & Status\midrule
**Parser & `pulldown-cmark & Emits syntax events & ✅ 

**Translator & `marksmen-core & Maps AST to Typst & ✅ 

**Compiler & `typst & Typesets elements & ✅ 

**Exporter & `typst-pdf & Emits binary format & ✅ 

\bottomrule
\end{longtable}


### 2.1 CSS & HTML Handling


Typst is an analytical typesetting engine, **not a web browser. It cannot execute CSS or complex DOM layouts. However, the translator enforces semantic mapping for structural inline HTML subsets:


- **Underline: This text is underlined via `\textless{}u\textgreater{}
- **Subscripts: H2O via `\textless{}sub\textgreater{}
- **Superscripts: E = mc2 via `\textless{}sup\textgreater{}
- **Colors: This text is colored via `\textless{}span style="color: ..."\textgreater{}
- **Line breaks: Forced HTML break  is translated correctly.



*Note: Arbitrary `\textless{}div style="padding: 2px"\textgreater{} blocks are ignored, preserving architectural invariants against empirical DOM approximations.


## 3. Mathematical Typesetting


True native math (without KaTeX SVG hacks): 


\[

\mathbf{J} = \nabla \times \mathbf{H} = \sigma \mathbf{E} + \frac{\partial \mathbf{D}}{\partial t}

\]



Inline equations like $\lim_{x \to \infty} f(x)$ blend seamlessly with the baseline.


## 4. Linked SVG Figures


Instead of relying on web-based JS to render graphics, Typst handles `SVG vector embeddings natively without rasterization:


\begin{figure}[h]
\centering
\includegraphics[width=\textwidth]{./diagram1.svg}
Architecture Diagram\end{figure}



## 5. Native Diagram Rendering (`marksmen-mermaid)

> 

**Pure-Rust Mermaid Translation: 

Executing standard `mermaid.js typically requires evaluating an entire web runtime (V8/Browser) with empirical DOM measurement APIs, which violates our offline-first pure-Rust correctness guarantees.


*Instead, `marksmen relies on a mathematically-verified `marksmen-mermaid layout engine implementing the Sugiyama Framework. It parses this raw Mermaid block offline and renders it deterministically into Typst spatial vectors!



```[language=mermaid]
graph TD
    A[Markdown AST] --\textgreater{} B{Marksmen Core}
    B --\textgreater{} C[Typst Vectors]
    C --\textgreater{} D(PDF Output)
    C --\textgreater{} E(HTML5 Output)
```


## Appendix: Mathematical Justifications


Because `marksmen leverages Typst, we can seamlessly write $e^(i pi) + 1 = 0$ in the middle of text, and be confident that it renders beautifully on a completely separated page.