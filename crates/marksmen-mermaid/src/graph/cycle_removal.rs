//! DFS algorithm mathematically guaranteeing graph acyclicity.
//!
//! Reverses edges that create a back-edge to guarantee a perfect DAG.

use super::directed_graph::DirectedGraph;
use rustc_hash::FxHashSet;

/// Analyzes a DirectedGraph and in-place reverses any back-edges that create cycles.
/// Employs a standard DFS state traversal.
pub fn remove_cycles(graph: &mut DirectedGraph) {
    let mut visited = FxHashSet::default();
    let mut stacked = FxHashSet::default();
    let mut edges_to_reverse = Vec::new();

    // Since the graph might be disconnected, we iterate over all nodes
    let node_ids: Vec<String> = graph.nodes().keys().cloned().collect();

    for start_node in node_ids {
        if !visited.contains(&start_node) {
            dfs_cycle_find(
                &start_node,
                graph,
                &mut visited,
                &mut stacked,
                &mut edges_to_reverse,
            );
        }
    }

    // Mathematically reverse any identified back-edges
    // to strictly preserve connectivity while eliminating cycles.
    for (from, to) in edges_to_reverse {
        if let Some(edge) = graph
            .edges_mut()
            .iter_mut()
            .find(|e| e.from == from && e.to == to)
        {
            edge.from = to.clone();
            edge.to = from.clone();

            // Note: Adjacency list must be rebuilt after this operation
            // This is handled by a separate function
        }
    }

    graph.rebuild_adjacency();
}

fn dfs_cycle_find(
    node: &str,
    graph: &DirectedGraph,
    visited: &mut FxHashSet<String>,
    stacked: &mut FxHashSet<String>,
    edges_to_reverse: &mut Vec<(String, String)>,
) {
    visited.insert(node.to_string());
    stacked.insert(node.to_string());

    for edge in graph.out_edges(node) {
        let neighbor = &edge.to;
        if !visited.contains(neighbor) {
            dfs_cycle_find(neighbor, graph, visited, stacked, edges_to_reverse);
        } else if stacked.contains(neighbor) {
            // Cycle detected: `node` -> `neighbor` forms a back-edge.
            // Mark for mathematical reversal.
            edges_to_reverse.push((node.to_string(), neighbor.to_string()));
        }
    }

    stacked.remove(node);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::directed_graph::DirectedGraph;
    use crate::parsing::lexer::EdgeStyle;

    #[test]
    fn DFS_reverses_mathematical_cycle() {
        let mut graph = DirectedGraph::new("TD".to_string());
        graph.add_node("A".to_string(), None, None, None);
        graph.add_node("B".to_string(), None, None, None);
        graph.add_node("C".to_string(), None, None, None);

        // A -> B -> C -> A (Cycle)
        graph.add_edge(
            "A".to_string(),
            "B".to_string(),
            EdgeStyle::SolidArrow,
            None,
        );
        graph.add_edge(
            "B".to_string(),
            "C".to_string(),
            EdgeStyle::SolidArrow,
            None,
        );
        graph.add_edge(
            "C".to_string(),
            "A".to_string(),
            EdgeStyle::SolidArrow,
            None,
        ); // Back-edge

        remove_cycles(&mut graph);

        // A -> B and B -> C should remain
        assert!(graph.edges().iter().any(|e| e.from == "A" && e.to == "B"));
        assert!(graph.edges().iter().any(|e| e.from == "B" && e.to == "C"));

        // C -> A should be mathematically reversed to A -> C
        assert!(!graph.edges().iter().any(|e| e.from == "C" && e.to == "A"));
        assert!(graph.edges().iter().any(|e| e.from == "A" && e.to == "C"));
    }
}
