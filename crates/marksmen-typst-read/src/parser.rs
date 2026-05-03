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
        SyntaxKind::SetRule | SyntaxKind::ShowRule | SyntaxKind::Hash => {
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
