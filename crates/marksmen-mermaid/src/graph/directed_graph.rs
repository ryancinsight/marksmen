//! Formal directed graph representation for topology extraction.

use rustc_hash::FxHashMap;

/// Mathematical representation of a flowchart.
#[derive(Debug, Clone, Default)]
pub struct DirectedGraph {
    pub direction: String,
    nodes: FxHashMap<String, NodeDetails>,
    edges: Vec<Edge>,
    pub subgraphs: Vec<Subgraph>,
    adjacency: FxHashMap<String, Vec<usize>>, // Node ID -> Edge indices
}

#[derive(Debug, Clone)]
pub struct NodeDetails {
    pub id: String,
    pub label: String,
    pub shape: Option<crate::parsing::lexer::NodeShape>,
    pub style: StyleSpec,
    // Sugiyama geometry bounds (width/height calculated later based on text)
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub style: crate::parsing::lexer::EdgeStyle,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleSpec {
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub stroke_width: Option<String>,
    pub stroke_dasharray: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Subgraph {
    pub title: String,
    pub nodes: Vec<String>,
    pub parent_title: Option<String>,
    pub depth: usize,
}

impl DirectedGraph {
    pub fn new(direction: String) -> Self {
        Self {
            direction,
            nodes: FxHashMap::default(),
            edges: Vec::new(),
            subgraphs: Vec::new(),
            adjacency: FxHashMap::default(),
        }
    }

    pub fn add_node(
        &mut self,
        id: String,
        label: Option<String>,
        shape: Option<crate::parsing::lexer::NodeShape>,
        style: Option<StyleSpec>,
    ) {
        if !self.nodes.contains_key(&id) {
            let label_str = sanitize_label(label.unwrap_or_else(|| id.clone()));
            // Base empirical geometry calculation placeholder (will be rigid in layout phase)
            let width = (label_str.len() as f64 * 8.0) + 20.0;
            let height = 30.0;
            
            self.nodes.insert(id.clone(), NodeDetails {
                id,
                label: label_str,
                shape,
                style: style.unwrap_or_default(),
                width,
                height,
            });
        } else if let Some(l) = label {
            // Update label if it wasn't defined earlier
            if let Some(node) = self.nodes.get_mut(&id) {
                if node.label == node.id {
                    node.label = sanitize_label(l);
                    if shape.is_some() {
                        node.shape = shape;
                    }
                }
                if let Some(new_style) = style {
                    merge_style(&mut node.style, &new_style);
                }
            }
        } else if let (Some(node), Some(new_style)) = (self.nodes.get_mut(&id), style) {
            merge_style(&mut node.style, &new_style);
        }
    }

    pub fn add_edge(&mut self, from: String, to: String, style: crate::parsing::lexer::EdgeStyle, label: Option<String>) {
        let edge_idx = self.edges.len();
        self.edges.push(Edge {
            from: from.clone(),
            to: to.clone(),
            style,
            label,
        });

        self.adjacency.entry(from).or_insert_with(Vec::new).push(edge_idx);
    }

    pub fn rebuild_adjacency(&mut self) {
        self.adjacency.clear();
        for (idx, edge) in self.edges.iter().enumerate() {
            self.adjacency.entry(edge.from.clone()).or_insert_with(Vec::new).push(idx);
        }
    }

    pub fn nodes(&self) -> &FxHashMap<String, NodeDetails> {
        &self.nodes
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn edges_mut(&mut self) -> &mut Vec<Edge> {
        &mut self.edges
    }

    pub fn out_edges(&self, node_id: &str) -> Vec<&Edge> {
        self.adjacency.get(node_id)
            .map(|indices| indices.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }
}

fn sanitize_label(label: String) -> String {
    label
        .replace("<br/>", " ")
        .replace("<br />", " ")
        .replace("<br>", " ")
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

/// Converts an AST into a logical DirectedGraph.
pub fn ast_to_graph(ast: crate::parsing::parser::Ast) -> DirectedGraph {
    let mut graph = DirectedGraph::new(ast.direction.clone());
    graph.subgraphs = ast.subgraphs.iter().map(|s| Subgraph {
        title: s.title.clone(),
        nodes: s.nodes.clone(),
        parent_title: s.parent_title.clone(),
        depth: s.depth,
    }).collect();

    for stmt in ast.statements {
        match stmt {
            crate::parsing::parser::Statement::Node(n) => {
                graph.add_node(n.id, n.label, n.shape, n.style);
            }
            crate::parsing::parser::Statement::Edge(e) => {
                graph.add_node(e.from.id.clone(), e.from.label, e.from.shape, e.from.style);
                graph.add_node(e.to.id.clone(), e.to.label, e.to.shape, e.to.style);
                graph.add_edge(e.from.id, e.to.id, e.style, e.label);
            }
            crate::parsing::parser::Statement::Chain(nodes, links) => {
                for i in 0..nodes.len() {
                    let n = &nodes[i];
                    graph.add_node(n.id.clone(), n.label.clone(), n.shape.clone(), n.style.clone());
                    
                    if i > 0 {
                        let prev = &nodes[i - 1];
                        let link = &links[i - 1];
                        graph.add_edge(prev.id.clone(), n.id.clone(), link.style.clone(), link.label.clone());
                    }
                }
            }
        }
    }

    graph
}
