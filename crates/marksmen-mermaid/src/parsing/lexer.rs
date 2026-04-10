//! Lexical analysis for Mermaid flowchart syntax.

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// `graph` or `flowchart`
    KeywordGraph,
    /// `TB`, `TD`, `BT`, `RL`, `LR`
    Direction(String),
    /// A node identifier (e.g., `A`, `Node1`)
    Identifier(String),
    /// A node label with its shape (`[Label]`, `(Label)`, `{Label}`)
    NodeLabel {
        text: String,
        shape: NodeShape,
    },
    /// An edge without a label (`-->`, `---`)
    Edge(EdgeStyle),
    /// An edge with an attached label.
    LabeledEdge {
        style: EdgeStyle,
        label: String,
    },
    /// Newline or semicolon indicating end of statement
    StatementTerminator,
    /// End of input
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeShape {
    /// `[Text]`
    Square,
    /// `(Text)`
    Round,
    /// `((Text))`
    Circle,
    /// `{Text}`
    Rhombus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeStyle {
    /// `---`
    SolidLine,
    /// `-->`
    SolidArrow,
    /// `-.->`
    DottedArrow,
    /// `==>`
    ThickArrow,
}

#[derive(Debug, Error)]
pub enum LexerError {
    #[error("Unexpected character: '{0}'")]
    UnexpectedChar(char),
    #[error("Unterminated string or label")]
    UnterminatedLabel,
}

/// Tokenizes a Mermaid flowchart string.
pub fn lex(input: &str) -> Result<Vec<Token>, LexerError> {
    let mut tokens = Vec::new();
    let chars = input.chars().collect::<Vec<_>>();
    let mut pos = 0;

    while pos < chars.len() {
        let c = chars[pos];

        if c.is_whitespace() {
            if c == '\n' {
                // Deduplicate terminators
                if tokens.last() != Some(&Token::StatementTerminator) {
                    tokens.push(Token::StatementTerminator);
                }
            }
            pos += 1;
            continue;
        }

        if c == ';' {
            if tokens.last() != Some(&Token::StatementTerminator) {
                tokens.push(Token::StatementTerminator);
            }
            pos += 1;
            continue;
        }

        // Handle class modifiers `:::`
        if pos + 2 < chars.len() && chars[pos] == ':' && chars[pos+1] == ':' && chars[pos+2] == ':' {
            pos += 3;
            // skip the class name until whitespace
            while pos < chars.len() && !chars[pos].is_whitespace() {
                pos += 1;
            }
            continue;
        }

        // Handle keywords and identifiers
        if c.is_alphabetic() {
            let start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                pos += 1;
            }
            let word: String = chars[start..pos].iter().collect();

            match word.as_str() {
                "graph" | "flowchart" | "sequenceDiagram" => tokens.push(Token::KeywordGraph),
                "TB" | "TD" | "BT" | "RL" | "LR" => tokens.push(Token::Direction(word)),
                "classDef" | "class" | "direction" | "participant" | "Note" | "note" | "loop" | "alt" | "else" | "activate" | "deactivate" => {
                    // Skip entirely configuration lines and unsupported sequence nodes
                    while pos < chars.len() && chars[pos] != '\n' && chars[pos] != ';' {
                        pos += 1;
                    }
                },
                "subgraph" => {
                    // Subgraph start line
                    while pos < chars.len() && chars[pos] != '\n' {
                        pos += 1;
                    }
                },
                "end" => {
                    // Subgraph or sequence termination - ignore
                },
                _ => tokens.push(Token::Identifier(word)),
            }
            continue;
        }

        // Handle edges (e.g., -->, ---, ==>, -.->, <-->, <-->)
        if c == '-' || c == '=' || c == '.' {
            let edge_token = lex_edge_token(&chars, &mut pos);
            tokens.push(edge_token);
            continue;
        }

        if c == '<' {
            let edge_token = lex_edge_token(&chars, &mut pos);
            tokens.push(edge_token);
            continue;
        }

        // Handle Node Labels e.g., [Label]
        if c == '[' {
            pos += 1;
            let start = pos;
            while pos < chars.len() && chars[pos] != ']' {
                pos += 1;
            }
            if pos >= chars.len() {
                return Err(LexerError::UnterminatedLabel);
            }
            let text: String = chars[start..pos].iter().collect();
            tokens.push(Token::NodeLabel { text, shape: NodeShape::Square });
            pos += 1;
            continue;
        }

        if c == '(' {
            if pos + 1 < chars.len() && chars[pos + 1] == '(' {
                pos += 2;
                let start = pos;
                while pos + 1 < chars.len() && !(chars[pos] == ')' && chars[pos + 1] == ')') {
                    pos += 1;
                }
                if pos + 1 >= chars.len() {
                    return Err(LexerError::UnterminatedLabel);
                }
                let text: String = chars[start..pos].iter().collect();
                tokens.push(Token::NodeLabel { text, shape: NodeShape::Circle });
                pos += 2;
            } else {
                pos += 1;
                let start = pos;
                while pos < chars.len() && chars[pos] != ')' {
                    pos += 1;
                }
                if pos >= chars.len() {
                    return Err(LexerError::UnterminatedLabel);
                }
                let text: String = chars[start..pos].iter().collect();
                tokens.push(Token::NodeLabel { text, shape: NodeShape::Round });
                pos += 1;
            }
            continue;
        }

        if c == '{' {
            pos += 1;
            let start = pos;
            while pos < chars.len() && chars[pos] != '}' {
                pos += 1;
            }
            if pos >= chars.len() {
                return Err(LexerError::UnterminatedLabel);
            }
            let text: String = chars[start..pos].iter().collect();
            tokens.push(Token::NodeLabel { text, shape: NodeShape::Rhombus });
            pos += 1;
            continue;
        }

        // Ignore unknown chars for now
        pos += 1;
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}

fn lex_edge_token(chars: &[char], pos: &mut usize) -> Token {
    let start = *pos;
    while *pos < chars.len() && (chars[*pos] == '<' || chars[*pos] == '-' || chars[*pos] == '=' || chars[*pos] == '.' || chars[*pos] == '>') {
        *pos += 1;
    }
    let leading: String = chars[start..*pos].iter().collect();

    while *pos < chars.len() && chars[*pos].is_whitespace() && chars[*pos] != '\n' {
        *pos += 1;
    }

    let mut label = None;

    if *pos < chars.len() && chars[*pos] == '|' {
        *pos += 1;
        let label_start = *pos;
        while *pos < chars.len() && chars[*pos] != '|' {
            *pos += 1;
        }
        let text: String = chars[label_start..(*pos).min(chars.len())].iter().collect();
        label = Some(text.trim().to_string());
        if *pos < chars.len() && chars[*pos] == '|' {
            *pos += 1;
        }
    } else {
        let suffixes = ["-->", "==>", "-.->", "---", "<-->", "<==>"];
        let mut probe = *pos;
        while probe < chars.len() && chars[probe] != '\n' {
            let rest: String = chars[probe..].iter().collect();
            if let Some(suffix) = suffixes.iter().find(|s| rest.starts_with(**s)) {
                let text: String = chars[*pos..probe].iter().collect();
                if !text.trim().is_empty() {
                    label = Some(text.trim().to_string());
                    *pos = probe + suffix.len();
                }
                break;
            }
            probe += 1;
        }
    }

    let full = leading.clone();
    let style = if full.contains('=') {
        EdgeStyle::ThickArrow
    } else if full.contains('.') {
        EdgeStyle::DottedArrow
    } else if full.contains('>') {
        EdgeStyle::SolidArrow
    } else {
        EdgeStyle::SolidLine
    };

    match label {
        Some(text) => Token::LabeledEdge { style, label: text },
        None => Token::Edge(style),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_basic_graph() {
        let input = "graph TD\n  A[Start] --> B(End)";
        let tokens = lex(input).unwrap();
        
        assert_eq!(tokens[0], Token::KeywordGraph);
        assert_eq!(tokens[1], Token::Direction("TD".to_string()));
        assert_eq!(tokens[2], Token::StatementTerminator);
        assert_eq!(tokens[3], Token::Identifier("A".to_string()));
        assert_eq!(tokens[4], Token::NodeLabel { text: "Start".to_string(), shape: NodeShape::Square });
        assert_eq!(tokens[5], Token::Edge(EdgeStyle::SolidArrow));
        assert_eq!(tokens[6], Token::Identifier("B".to_string()));
        assert_eq!(tokens[7], Token::NodeLabel { text: "End".to_string(), shape: NodeShape::Round });
        assert_eq!(tokens[8], Token::Eof);
    }

    #[test]
    fn lex_circle_and_labeled_edge() {
        let input = "flowchart LR\nA((Start)) -->|Label| B[End]";
        let tokens = lex(input).unwrap();

        assert_eq!(tokens[0], Token::KeywordGraph);
        assert_eq!(tokens[1], Token::Direction("LR".to_string()));
        assert_eq!(tokens[3], Token::Identifier("A".to_string()));
        assert_eq!(tokens[4], Token::NodeLabel { text: "Start".to_string(), shape: NodeShape::Circle });
        assert_eq!(tokens[5], Token::LabeledEdge { style: EdgeStyle::SolidArrow, label: "Label".to_string() });
        assert_eq!(tokens[6], Token::Identifier("B".to_string()));
    }

    #[test]
    fn lex_spaced_thick_labeled_edge() {
        let input = "flowchart LR\nA == OpenIGTLink TLS 256-Bit ==> B";
        let tokens = lex(input).unwrap();
        assert_eq!(tokens[3], Token::Identifier("A".to_string()));
        assert_eq!(tokens[4], Token::LabeledEdge {
            style: EdgeStyle::ThickArrow,
            label: "OpenIGTLink TLS 256-Bit".to_string(),
        });
        assert_eq!(tokens[5], Token::Identifier("B".to_string()));
    }
}
