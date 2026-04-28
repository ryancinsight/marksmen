//! Abstract Syntax Tree (AST) definition and parser.

use super::lexer::{lex, LexerError, Token};
use crate::graph::directed_graph::StyleSpec;
use regex::Regex;
use rustc_hash::FxHashMap;
use thiserror::Error;

/// A parsed Mermaid flowchart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ast {
    /// Graph direction (e.g., "TD", "LR")
    pub direction: String,
    /// Statements in the graph (node declarations, edges, etc.)
    pub statements: Vec<Statement>,
    pub subgraphs: Vec<SubgraphDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// A standalone node definition (e.g., `A[Label]`)
    Node(NodeDef),
    /// A single connection `A --> B`
    Edge(EdgeDef),
    /// A chain of connections `A --> B --> C`
    Chain(Vec<NodeDef>, Vec<EdgeLink>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDef {
    pub id: String,
    pub label: Option<String>,
    pub shape: Option<super::lexer::NodeShape>,
    pub style: Option<StyleSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeDef {
    pub from: NodeDef,
    pub to: NodeDef,
    pub style: super::lexer::EdgeStyle,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeLink {
    pub style: super::lexer::EdgeStyle,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubgraphDef {
    pub title: String,
    pub nodes: Vec<String>,
    pub parent_title: Option<String>,
    pub depth: usize,
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Lexer error: {0}")]
    Lex(#[from] LexerError),
    #[error("Unexpected token: {0:?}")]
    UnexpectedToken(Token),
    #[error("Unexpected end of input")]
    UnexpectedEof,
    #[error("Graph must start with 'graph' or 'flowchart'")]
    MissingGraphKeyword,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    node_styles: FxHashMap<String, StyleSpec>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, node_styles: FxHashMap<String, StyleSpec>) -> Self {
        Self {
            tokens,
            pos: 0,
            node_styles,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.pos);
        self.pos += 1;
        token
    }

    pub fn parse(&mut self) -> Result<Ast, ParseError> {
        // Must start with `graph` keyword
        let first = self.advance().ok_or(ParseError::UnexpectedEof)?;
        if *first != Token::KeywordGraph {
            return Err(ParseError::MissingGraphKeyword);
        }

        // Must have direction
        let direction = match self.advance() {
            Some(Token::Direction(d)) => d.clone(),
            Some(t) => return Err(ParseError::UnexpectedToken(t.clone())),
            None => return Err(ParseError::UnexpectedEof),
        };

        // Skip to end of statement
        while let Some(t) = self.peek() {
            if *t == Token::StatementTerminator {
                self.advance();
            } else {
                break;
            }
        }

        let mut statements = Vec::new();

        while let Some(t) = self.peek() {
            match t {
                Token::Eof => break,
                Token::StatementTerminator => {
                    self.advance();
                    // consume consecutive blank lines
                    while let Some(Token::StatementTerminator) = self.peek() {
                        self.advance();
                    }
                }
                _ => {
                    let stmt = self.parse_statement()?;
                    statements.push(stmt);
                }
            }
        }

        Ok(Ast {
            direction,
            statements,
            subgraphs: Vec::new(),
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        // Read first node in the chain
        let mut nodes = vec![self.parse_node_def()?];
        let mut edges = Vec::new();

        // While there is an edge, keep reading chain
        while let Some(token) = self.peek() {
            let link = match token {
                Token::Edge(style) => EdgeLink {
                    style: style.clone(),
                    label: None,
                },
                Token::LabeledEdge { style, label } => EdgeLink {
                    style: style.clone(),
                    label: Some(label.clone()),
                },
                _ => break,
            };
            edges.push(link);
            self.advance();

            let next_node = self.parse_node_def()?;
            nodes.push(next_node);
        }

        if nodes.len() == 1 {
            Ok(Statement::Node(nodes.pop().unwrap()))
        } else if nodes.len() == 2 {
            let to = nodes.pop().unwrap();
            let from = nodes.pop().unwrap();
            let link = edges.pop().unwrap();
            Ok(Statement::Edge(EdgeDef {
                from,
                to,
                style: link.style,
                label: link.label,
            }))
        } else {
            Ok(Statement::Chain(nodes, edges))
        }
    }

    fn parse_node_def(&mut self) -> Result<NodeDef, ParseError> {
        let id_token = self.advance().ok_or(ParseError::UnexpectedEof)?;

        let id = match id_token {
            Token::Identifier(name) => name.clone(),
            _ => return Err(ParseError::UnexpectedToken(id_token.clone())),
        };

        let mut label = None;
        let mut shape = None;

        if let Some(Token::NodeLabel {
            text,
            shape: node_shape,
        }) = self.peek()
        {
            label = Some(text.clone());
            shape = Some(node_shape.clone());
            self.advance();
        }

        Ok(NodeDef {
            style: self.node_styles.get(&id).cloned(),
            id,
            label,
            shape,
        })
    }
}

pub fn parse(input: &str) -> Result<Ast, ParseError> {
    let node_styles = extract_node_styles(input);
    let subgraphs = extract_subgraphs(input);
    let normalized = normalize_flowchart_source(input);
    let tokens = lex(&normalized)?;
    let mut parser = Parser::new(tokens, node_styles);
    let mut ast = parser.parse()?;
    ast.subgraphs = subgraphs;
    Ok(ast)
}

fn normalize_flowchart_source(input: &str) -> String {
    let mut normalized_lines = Vec::new();

    for raw_line in input.lines() {
        let trimmed = raw_line.trim();

        if trimmed.is_empty() {
            normalized_lines.push(String::new());
            continue;
        }

        if trimmed.starts_with("%%")
            || trimmed.starts_with("classDef ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("linkStyle ")
            || trimmed.starts_with("style ")
        {
            continue;
        }

        if trimmed.starts_with("subgraph ") || trimmed == "end" {
            continue;
        }

        let mut line = strip_class_annotations(raw_line);
        line = line.replace("<-->", "-->");
        line = line.replace("<==>", "==>");
        normalized_lines.push(line);
    }

    normalized_lines.join("\n")
}

fn strip_class_annotations(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < chars.len() {
        if i + 2 < chars.len() && chars[i] == ':' && chars[i + 1] == ':' && chars[i + 2] == ':' {
            i += 3;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-')
            {
                i += 1;
            }
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

fn extract_node_styles(input: &str) -> FxHashMap<String, StyleSpec> {
    let class_def_re = Regex::new(r"^\s*classDef\s+([A-Za-z0-9_-]+)\s+(.+?);?\s*$").unwrap();
    let class_stmt_re =
        Regex::new(r"^\s*class\s+([A-Za-z0-9_,\s-]+)\s+([A-Za-z0-9_-]+);?\s*$").unwrap();
    let inline_class_re = Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\s*(?:\[[^\]]*\]|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\})?:::([A-Za-z0-9_-]+)").unwrap();

    let mut class_defs: FxHashMap<String, StyleSpec> = FxHashMap::default();
    let mut node_classes: FxHashMap<String, Vec<String>> = FxHashMap::default();

    for line in input.lines() {
        if let Some(caps) = class_def_re.captures(line) {
            class_defs.insert(caps[1].to_string(), parse_style_properties(&caps[2]));
            continue;
        }

        if let Some(caps) = class_stmt_re.captures(line) {
            let class_name = caps[2].trim().to_string();
            for node_id in caps[1]
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                node_classes
                    .entry(node_id.to_string())
                    .or_default()
                    .push(class_name.clone());
            }
        }

        for caps in inline_class_re.captures_iter(line) {
            node_classes
                .entry(caps[1].to_string())
                .or_default()
                .push(caps[2].to_string());
        }
    }

    let mut node_styles = FxHashMap::default();
    for (node_id, classes) in node_classes {
        let mut style = StyleSpec::default();
        for class_name in classes {
            if let Some(class_style) = class_defs.get(&class_name) {
                merge_style(&mut style, class_style);
            }
        }
        if style != StyleSpec::default() {
            node_styles.insert(node_id, style);
        }
    }

    node_styles
}

fn parse_style_properties(input: &str) -> StyleSpec {
    let mut style = StyleSpec::default();
    for part in input.split(',') {
        let Some((key, value)) = part.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_end_matches(';').to_string();
        match key {
            "fill" => style.fill = Some(value),
            "stroke" => style.stroke = Some(value),
            "stroke-width" => style.stroke_width = Some(value),
            "stroke-dasharray" => style.stroke_dasharray = Some(value),
            "color" => style.color = Some(value),
            _ => {}
        }
    }
    style
}

fn merge_style(target: &mut StyleSpec, incoming: &StyleSpec) {
    if incoming.fill.is_some() {
        target.fill = incoming.fill.clone();
    }
    if incoming.stroke.is_some() {
        target.stroke = incoming.stroke.clone();
    }
    if incoming.stroke_width.is_some() {
        target.stroke_width = incoming.stroke_width.clone();
    }
    if incoming.stroke_dasharray.is_some() {
        target.stroke_dasharray = incoming.stroke_dasharray.clone();
    }
    if incoming.color.is_some() {
        target.color = incoming.color.clone();
    }
}

fn extract_subgraphs(input: &str) -> Vec<SubgraphDef> {
    #[derive(Debug)]
    struct SubgraphFrame {
        title: String,
        nodes: Vec<String>,
        depth: usize,
    }

    let node_re = Regex::new(
        r"\b([A-Za-z_][A-Za-z0-9_]*)\b\s*(?:\[[^\]]*\]|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\})?",
    )
    .unwrap();
    let reserved = ["graph", "flowchart", "subgraph", "end"];
    let mut stack: Vec<SubgraphFrame> = Vec::new();
    let mut subgraphs = Vec::new();

    for raw_line in input.lines() {
        let trimmed = raw_line.trim();
        if let Some(rest) = trimmed.strip_prefix("subgraph ") {
            stack.push(SubgraphFrame {
                title: rest.trim().trim_matches('"').to_string(),
                nodes: Vec::new(),
                depth: stack.len(),
            });
            continue;
        }
        if trimmed == "end" {
            if let Some(frame) = stack.pop() {
                let parent_title = stack.last().map(|parent| parent.title.clone());
                if let Some(parent) = stack.last_mut() {
                    for node_id in &frame.nodes {
                        if !parent.nodes.contains(node_id) {
                            parent.nodes.push(node_id.clone());
                        }
                    }
                }
                if !frame.nodes.is_empty() {
                    subgraphs.push(SubgraphDef {
                        title: frame.title,
                        nodes: frame.nodes,
                        parent_title,
                        depth: frame.depth,
                    });
                }
            }
            continue;
        }
        if stack.is_empty() {
            continue;
        }

        let node_ids: Vec<String> = node_re
            .captures_iter(trimmed)
            .filter_map(|caps| {
                let id = caps[1].to_string();
                if reserved.contains(&id.as_str()) {
                    None
                } else {
                    Some(id)
                }
            })
            .collect();

        if let Some(frame) = stack.last_mut() {
            for node_id in node_ids {
                if !frame.nodes.contains(&node_id) {
                    frame.nodes.push(node_id);
                }
            }
        }
    }

    subgraphs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_ast() {
        let input = "graph TD\n  A[Start] --> B(End)";
        let ast = parse(input).unwrap();

        assert_eq!(ast.direction, "TD");
        assert_eq!(ast.statements.len(), 1);

        match &ast.statements[0] {
            Statement::Edge(e) => {
                assert_eq!(e.from.id, "A");
                assert_eq!(e.from.label.as_deref(), Some("Start"));
                assert_eq!(e.to.id, "B");
                assert_eq!(e.to.label.as_deref(), Some("End"));
            }
            _ => panic!("Expected edge statement"),
        }
    }

    #[test]
    fn parse_flowchart_with_mermaid_directives() {
        let input = "flowchart LR\n    classDef local fill:#eef4ff\n    subgraph Test\n        A[Left]:::local -->|Labeled Link| B((Right)):::local\n    end";
        let ast = parse(input).unwrap();

        assert_eq!(ast.direction, "LR");
        assert_eq!(ast.statements.len(), 1);
        assert_eq!(ast.subgraphs.len(), 1);
        assert_eq!(ast.subgraphs[0].title, "Test");
        assert_eq!(ast.subgraphs[0].depth, 0);

        match &ast.statements[0] {
            Statement::Edge(e) => {
                assert_eq!(e.from.id, "A");
                assert_eq!(e.to.id, "B");
                assert_eq!(e.to.shape, Some(crate::parsing::lexer::NodeShape::Circle));
                assert_eq!(e.label.as_deref(), Some("Labeled Link"));
                assert_eq!(
                    e.from.style.as_ref().and_then(|s| s.fill.as_deref()),
                    Some("#eef4ff")
                );
            }
            _ => panic!("Expected edge statement"),
        }
    }

    #[test]
    fn parse_nested_subgraphs() {
        let input = "flowchart LR\nsubgraph Outer\n    subgraph Inner\n        A[Alpha]\n    end\n    B[Beta]\nend\nA --> B";
        let ast = parse(input).unwrap();

        assert_eq!(ast.subgraphs.len(), 2);
        let outer = ast.subgraphs.iter().find(|s| s.title == "Outer").unwrap();
        let inner = ast.subgraphs.iter().find(|s| s.title == "Inner").unwrap();
        assert_eq!(outer.parent_title, None);
        assert_eq!(outer.depth, 0);
        assert!(outer.nodes.contains(&"A".to_string()));
        assert!(outer.nodes.contains(&"B".to_string()));
        assert_eq!(inner.parent_title.as_deref(), Some("Outer"));
        assert_eq!(inner.depth, 1);
        assert_eq!(inner.nodes, vec!["A".to_string()]);
    }
}
