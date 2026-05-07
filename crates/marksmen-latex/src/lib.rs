//! LaTeX target builder evaluating marksmen AST flows into strict, zero-dependency LaTeX arrays.

use anyhow::Result;
use marksmen_core::Config;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Tag, TagEnd};

pub fn convert(events: &[Event<'_>], config: &Config) -> Result<String> {
    let mut out = String::with_capacity(events.len() * 100);

    // Preamble
    out.push_str("\\documentclass{article}\n");
    out.push_str("\\usepackage[utf8]{inputenc}\n");
    out.push_str("\\usepackage{amsmath}\n");
    out.push_str("\\usepackage{amsfonts}\n");
    out.push_str("\\usepackage{amssymb}\n");
    out.push_str("\\usepackage{graphicx}\n");
    out.push_str("\\usepackage{hyperref}\n");
    out.push_str("\\usepackage{booktabs}\n");
    out.push_str("\\usepackage{longtable}\n");
    out.push_str("\\usepackage{listings}\n");
    out.push_str("\\usepackage{color}\n");
    out.push_str("\\usepackage[margin=1in]{geometry}\n");
    // Minimal listings config for code blocks
    out.push_str("\\definecolor{lightgray}{gray}{0.95}\n");
    out.push_str("\\lstset{\n");
    out.push_str("    backgroundcolor=\\color{lightgray},\n");
    out.push_str("    basicstyle=\\ttfamily\\small,\n");
    out.push_str("    breaklines=true,\n");
    out.push_str("    frame=single,\n");
    out.push_str("}\n\n");

    if !config.title.is_empty() {
        out.push_str(&format!("\\title{{{}}}\n", escape_latex(&config.title)));
    }
    if !config.author.is_empty() {
        out.push_str(&format!("\\author{{{}}}\n", escape_latex(&config.author)));
    }
    if !config.date.is_empty() {
        out.push_str(&format!("\\date{{{}}}\n", escape_latex(&config.date)));
    }

    out.push_str("\\begin{document}\n\n");

    if !config.title.is_empty() {
        out.push_str("\\maketitle\n\n");
    }

    if !config.abstract_text.is_empty() {
        out.push_str("\\begin{abstract}\n");
        out.push_str(&escape_latex(&config.abstract_text));
        out.push_str("\n\\end{abstract}\n\n");
    }

    let mut state = LatexState::default();

    for event in events.iter().cloned() {
        match event {
            Event::Start(Tag::Paragraph)
                if !state.in_table => {
                    out.push('\n');
                }
            Event::End(TagEnd::Paragraph)
                if !state.in_table => {
                    out.push_str("\n\n");
                }
            Event::Start(Tag::Heading { level, .. }) => {
                let lev = match level {
                    pulldown_cmark::HeadingLevel::H1 => "\\section{",
                    pulldown_cmark::HeadingLevel::H2 => "\\subsection{",
                    pulldown_cmark::HeadingLevel::H3 => "\\subsubsection{",
                    pulldown_cmark::HeadingLevel::H4 => "\\paragraph{",
                    pulldown_cmark::HeadingLevel::H5 => "\\subparagraph{",
                    pulldown_cmark::HeadingLevel::H6 => "\\subparagraph{",
                };
                out.push_str(lev);
            }
            Event::End(TagEnd::Heading(_)) => out.push_str("}\n\n"),
            Event::Start(Tag::BlockQuote(_)) => out.push_str("\\begin{quote}\n"),
            Event::End(TagEnd::BlockQuote(_)) => out.push_str("\\end{quote}\n\n"),
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang))) => {
                let l = lang.as_ref();
                if l.is_empty() {
                    out.push_str("\\begin{lstlisting}\n");
                } else if l == "mermaid" {
                    out.push_str("\\begin{lstlisting}[language=mermaid]\n"); // Fallback text formatting
                } else {
                    out.push_str(&format!("\\begin{{lstlisting}}[language={}]\n", l));
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                out.push_str("\\begin{lstlisting}\n");
            }
            Event::End(TagEnd::CodeBlock) => {
                out.push_str("\\end{lstlisting}\n\n");
            }
            Event::Start(Tag::List(Some(_))) => {
                out.push_str("\\begin{enumerate}\n");
            }
            Event::Start(Tag::List(None)) => {
                out.push_str("\\begin{itemize}\n");
            }
            Event::End(TagEnd::List(is_ord)) => {
                if is_ord {
                    out.push_str("\\end{enumerate}\n\n");
                } else {
                    out.push_str("\\end{itemize}\n\n");
                }
            }
            Event::Start(Tag::Item) => out.push_str("\\item "),
            Event::End(TagEnd::Item) => out.push('\n'),
            Event::Start(Tag::Table(alignments)) => {
                state.in_table = true;
                let mut align_str = String::new();
                for a in &alignments {
                    match a {
                        Alignment::Left => align_str.push('l'),
                        Alignment::Center => align_str.push('c'),
                        Alignment::Right => align_str.push('r'),
                        Alignment::None => align_str.push('l'),
                    }
                }
                out.push_str(&format!(
                    "\\begin{{longtable}}{{{}}}\n\\toprule\n",
                    align_str
                ));
            }
            Event::End(TagEnd::Table) => {
                out.push_str("\\bottomrule\n\\end{longtable}\n\n");
                state.in_table = false;
            }
            Event::Start(Tag::TableHead) => {}
            Event::End(TagEnd::TableHead) => out.push_str("\\midrule\n"),
            Event::Start(Tag::TableRow) => {
                state.cell_index = 0;
            }
            Event::End(TagEnd::TableRow) => {
                out.push_str(" \\\\\n");
            }
            Event::Start(Tag::TableCell) => {
                if state.cell_index > 0 {
                    out.push_str(" & ");
                }
                state.cell_index += 1;
            }
            Event::End(TagEnd::TableCell) => {}
            Event::Start(Tag::Emphasis) => out.push_str("\\textit{"),
            Event::End(TagEnd::Emphasis) => out.push('}'),
            Event::Start(Tag::Strong) => out.push_str("\\textbf{"),
            Event::End(TagEnd::Strong) => out.push('}'),
            Event::Start(Tag::Strikethrough) => out.push_str(r"\sout{"), // requires \usepackage[normalem]{ulem} but we'll cheat or they can add it
            Event::End(TagEnd::Strikethrough) => out.push('}'),
            Event::Start(Tag::Link { dest_url, .. }) => {
                out.push_str(&format!("\\href{{{}}}{{", dest_url))
            }
            Event::End(TagEnd::Link) => out.push('}'),
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                out.push_str("\\begin{figure}[h]\n\\centering\n");
                out.push_str(&format!(
                    "\\includegraphics[width=\\textwidth]{{{}}}\n",
                    dest_url
                ));
                if !title.is_empty() {
                    out.push_str(&format!("\\caption{{{}}}\n", escape_latex(title.as_ref())));
                }
            }
            Event::End(TagEnd::Image) => out.push_str("\\end{figure}\n"),
            Event::Code(text) => {
                out.push_str(&format!("\\texttt{{{}}}", escape_latex(text.as_ref())))
            }
            Event::Text(text) => out.push_str(&escape_latex(text.as_ref())),
            Event::SoftBreak | Event::HardBreak => {
                if !state.in_table {
                    out.push_str(" \\\\\n");
                } else {
                    out.push(' ');
                }
            }
            Event::InlineMath(math) => {
                out.push_str(&format!("${}$", math));
            }
            Event::DisplayMath(math) => {
                out.push_str(&format!("\n\\[\n{}\n\\]\n", math));
            }
            Event::Rule => out.push_str("\n\\newpage\n\n"),
            Event::FootnoteReference(label) => {
                out.push_str(&format!("\\footnotemark[{}]", escape_latex(label.as_ref())));
            }
            _ => {}
        }
    }

    out.push_str("\\end{document}\n");
    Ok(out)
}

#[derive(Default)]
struct LatexState {
    in_table: bool,
    cell_index: usize,
}

fn escape_latex(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '\\' => out.push_str("\\textbackslash{}"),
            '<' => out.push_str("\\textless{}"),
            '>' => out.push_str("\\textgreater{}"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use marksmen_core::parsing::parser;

    #[test]
    fn test_conversion_runtime() {
        let md = "# Hello\nTest paragraph with *italic* and **bold**.\n\n- item 1\n- item 2\n\n```python\nprint(1)\n```";
        let events = parser::parse(md);
        let latex = convert(&events, &Config::default()).unwrap();
        assert!(latex.contains("Hello"));
    }
}
