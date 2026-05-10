use anyhow::Result;
use typst::syntax::{SyntaxKind, SyntaxNode, ast};

/// Parse Typst source text and map its syntactical primitives back into a
/// Markdown string mapping.
pub fn parse_typst(text: &str) -> Result<String> {
    let node = typst::syntax::parse(text);
    let mut markdown = String::with_capacity(text.len());
    traverse_node(&node, &mut markdown);
    Ok(markdown)
}

fn traverse_node(node: &SyntaxNode, output: &mut String) {
    let kind = node.kind();

    match kind {
        SyntaxKind::Text => {
            output.push_str(node.text().as_str());
        }
        SyntaxKind::Space => {
            output.push_str(node.text().as_str());
        }
        SyntaxKind::Parbreak => {
            output.push_str("\n\n");
        }
        SyntaxKind::Strong => {
            output.push_str("**");
            for child in node.children() {
                traverse_node(child, output);
            }
            output.push_str("**");
        }
        SyntaxKind::Emph => {
            output.push('*');
            for child in node.children() {
                traverse_node(child, output);
            }
            output.push('*');
        }
        SyntaxKind::Heading => {
            let level = if let Some(heading) = node.cast::<ast::Heading>() {
                heading.depth().get()
            } else {
                1
            };

            output.push('\n');
            output.push_str(&"#".repeat(level));
            output.push(' ');

            for child in node.children() {
                if child.kind() != SyntaxKind::HeadingMarker && child.kind() != SyntaxKind::Space {
                    traverse_node(child, output);
                }
            }
            output.push('\n');
        }
        SyntaxKind::Math => {
            if let Some(math) = node.cast::<ast::Equation>() {
                let formula = node.text().trim_matches('$').trim();
                if math.block() {
                    output.push_str(&format!("\n$$ {} $$\n", formula));
                } else {
                    output.push_str(&format!("${}$", formula));
                }
            }
        }
        SyntaxKind::CodeBlock => {
            // Incomplete CodeBlock mapping placeholder
            output.push_str("```\n");
            output.push_str(node.text().as_str());
            output.push_str("\n```\n");
        }
        SyntaxKind::EnumItem => {
            for child in node.children() {
                traverse_node(child, output);
            }
        }
        SyntaxKind::ListItem => {
            output.push_str("\n- ");
            for child in node.children() {
                if child.kind() != SyntaxKind::ListMarker && child.kind() != SyntaxKind::Space {
                    traverse_node(child, output);
                }
            }
        }
        SyntaxKind::TermItem | SyntaxKind::Markup | SyntaxKind::Equation => {
            for child in node.children() {
                traverse_node(child, output);
            }
        }
        SyntaxKind::FuncCall => {
            let mut func_name = "";
            let mut args_node = None;
            for child in node.children() {
                if child.kind() == SyntaxKind::Ident {
                    func_name = child.text().as_str();
                } else if child.kind() == SyntaxKind::Args {
                    args_node = Some(child);
                }
            }

            match func_name {
                "strong" => {
                    output.push_str("**");
                    if let Some(args) = args_node {
                        for arg_child in args.children() {
                            if arg_child.kind() == SyntaxKind::ContentBlock {
                                for content_child in arg_child.children() {
                                    traverse_node(content_child, output);
                                }
                            }
                        }
                    }
                    output.push_str("**");
                }
                "emph" => {
                    output.push('*');
                    if let Some(args) = args_node {
                        for arg_child in args.children() {
                            if arg_child.kind() == SyntaxKind::ContentBlock {
                                for content_child in arg_child.children() {
                                    traverse_node(content_child, output);
                                }
                            }
                        }
                    }
                    output.push('*');
                }
                "strike" => {
                    output.push_str("~~");
                    if let Some(args) = args_node {
                        for arg_child in args.children() {
                            if arg_child.kind() == SyntaxKind::ContentBlock {
                                for content_child in arg_child.children() {
                                    traverse_node(content_child, output);
                                }
                            }
                        }
                    }
                    output.push_str("~~");
                }
                "sub" => {
                    output.push_str("<sub>");
                    if let Some(args) = args_node {
                        for arg_child in args.children() {
                            if arg_child.kind() == SyntaxKind::ContentBlock {
                                for content_child in arg_child.children() {
                                    traverse_node(content_child, output);
                                }
                            }
                        }
                    }
                    output.push_str("</sub>");
                }
                "super" => {
                    output.push_str("<sup>");
                    if let Some(args) = args_node {
                        for arg_child in args.children() {
                            if arg_child.kind() == SyntaxKind::ContentBlock {
                                for content_child in arg_child.children() {
                                    traverse_node(content_child, output);
                                }
                            }
                        }
                    }
                    output.push_str("</sup>");
                }
                "link" => {
                    let mut url = String::new();
                    let mut text_output = String::new();
                    if let Some(args) = args_node {
                        for arg_child in args.children() {
                            if arg_child.kind() == SyntaxKind::Str {
                                url = arg_child.text().trim_matches('"').to_string();
                            } else if arg_child.kind() == SyntaxKind::ContentBlock {
                                for content_child in arg_child.children() {
                                    traverse_node(content_child, &mut text_output);
                                }
                            }
                        }
                    }
                    if text_output.is_empty() {
                        output.push_str(&format!("<{}>", url));
                    } else {
                        output.push_str(&format!("[{}]({})", text_output.trim(), url));
                    }
                }
                "image" => {
                    let mut url = String::new();
                    let mut alt = String::new();
                    if let Some(args) = args_node {
                        for arg_child in args.children() {
                            if arg_child.kind() == SyntaxKind::Str {
                                url = arg_child.text().trim_matches('"').to_string();
                            } else if arg_child.kind() == SyntaxKind::Named {
                                let mut is_alt = false;
                                for named_child in arg_child.children() {
                                    if named_child.kind() == SyntaxKind::Ident
                                        && named_child.text() == "alt"
                                    {
                                        is_alt = true;
                                    } else if is_alt && named_child.kind() == SyntaxKind::Str {
                                        alt = named_child.text().trim_matches('"').to_string();
                                    }
                                }
                            }
                        }
                    }
                    output.push_str(&format!("![{}]({})", alt, url));
                }
                "table" => {
                    if let Some(args) = args_node {
                        let mut aligns = Vec::new();
                        let mut cells = Vec::new();
                        let mut num_cols = 0;

                        for arg_child in args.children() {
                            if arg_child.kind() == SyntaxKind::Named {
                                let mut is_align = false;
                                let mut is_cols = false;
                                for named_child in arg_child.children() {
                                    if named_child.kind() == SyntaxKind::Ident {
                                        let text = named_child.text();
                                        if text == "align" {
                                            is_align = true;
                                        } else if text == "columns" {
                                            is_cols = true;
                                        }
                                    } else if is_align && named_child.kind() == SyntaxKind::Array {
                                        for arr_child in named_child.children() {
                                            if arr_child.kind() == SyntaxKind::Ident {
                                                aligns.push(arr_child.text().to_string());
                                            }
                                        }
                                    } else if is_cols && named_child.kind() == SyntaxKind::Array {
                                        // Just count the elements that aren't spaces/commas to get num_cols roughly
                                        for arr_child in named_child.children() {
                                            if arr_child.kind() != SyntaxKind::Space
                                                && arr_child.kind() != SyntaxKind::Comma
                                                && arr_child.kind() != SyntaxKind::LeftParen
                                                && arr_child.kind() != SyntaxKind::RightParen
                                            {
                                                num_cols += 1;
                                            }
                                        }
                                    }
                                }
                            } else if arg_child.kind() == SyntaxKind::ContentBlock {
                                let mut cell_text = String::new();
                                for content_child in arg_child.children() {
                                    traverse_node(content_child, &mut cell_text);
                                }
                                cells.push(cell_text.trim().to_string());
                            }
                        }

                        if num_cols == 0 && !cells.is_empty() {
                            num_cols = aligns.len().max(1);
                        }
                        if num_cols > 0 {
                            output.push('\n');
                            for (i, cell) in cells.iter().enumerate() {
                                if i % num_cols == 0 {
                                    output.push_str("| ");
                                }
                                output.push_str(cell);
                                output.push_str(" | ");
                                if (i + 1) % num_cols == 0 {
                                    output.push('\n');
                                    // If first row, emit header separator
                                    if i + 1 == num_cols {
                                        for j in 0..num_cols {
                                            let align =
                                                aligns.get(j).map(|s| s.as_str()).unwrap_or("auto");
                                            match align {
                                                "left" => output.push_str("| :--- "),
                                                "center" => output.push_str("| :---: "),
                                                "right" => output.push_str("| ---: "),
                                                _ => output.push_str("| --- "),
                                            }
                                        }
                                        output.push_str("|\n");
                                    }
                                }
                            }
                            if cells.len() % num_cols != 0 {
                                output.push('\n');
                            }
                        }
                    }
                }
                "align" | "block" | "pad" | "rect" | "highlight" | "text" => {
                    // Transparent wrappers: just pass through ContentBlocks
                    if let Some(args) = args_node {
                        for arg_child in args.children() {
                            if arg_child.kind() == SyntaxKind::ContentBlock {
                                for content_child in arg_child.children() {
                                    traverse_node(content_child, output);
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Ignore unknown functions to prevent preamble leaking
                }
            }
        }
        SyntaxKind::SetRule
        | SyntaxKind::ShowRule
        | SyntaxKind::Hash
        | SyntaxKind::LeftBracket
        | SyntaxKind::RightBracket => {
            // Ignore configuration preamble and AST control characters.
        }
        _ => {
            // Only emit raw text for unrecognized leaf nodes.
            // Do NOT recurse into unknown branch nodes — this prevents
            // preamble configuration (set rules, function calls, closures)
            // from leaking into the extracted markdown.
            if node.children().count() == 0 {
                let text = node.text().as_str();
                output.push_str(text);
            }
        }
    }
}
