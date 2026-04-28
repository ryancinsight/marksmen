//! Rank Assignment (Phase 1 of Sugiyama Framework)
//!
//! Mathematically partitions a DirectedGraph into discrete horizontal "ranks" or "layers"
//! via longest-path topological sorting.

use crate::graph::directed_graph::DirectedGraph;
use rustc_hash::FxHashMap;

/// Represents the graph subdivided into Y-levels.
#[derive(Debug, Clone)]
pub struct RankedGraph {
    pub direction: String,
    /// Ranks (Y-level) mapping to a list of Node IDs on that rank
    pub ranks: Vec<Vec<String>>,
    /// Original node details mapped by ID
    pub nodes: FxHashMap<String, crate::graph::directed_graph::NodeDetails>,
    /// Mathematically routed edges
    pub edges: Vec<crate::graph::directed_graph::Edge>,
    pub subgraphs: Vec<crate::graph::directed_graph::Subgraph>,
}

/// Assigns ranks to nodes using the longest-path algorithm prioritizing source nodes at Rank 0.
pub fn assign_ranks(graph: &DirectedGraph) -> RankedGraph {
    let mut in_degrees: FxHashMap<String, usize> =
        graph.nodes().keys().map(|k| (k.clone(), 0)).collect();
    let mut rank_map: FxHashMap<String, usize> = FxHashMap::default();

    // Calculate in-degrees
    for edge in graph.edges() {
        *in_degrees.entry(edge.to.clone()).or_insert(0) += 1;
    }

    // Identify sources (in-degree 0)
    let mut queue: Vec<String> = in_degrees
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(k, _)| k.clone())
        .collect();

    // Base ranks at 0
    for q in &queue {
        rank_map.insert(q.clone(), 0);
    }

    // Topological sort & longest path
    while let Some(u) = queue.pop() {
        let current_rank = *rank_map.get(&u).unwrap();

        for edge in graph.out_edges(&u) {
            let v = &edge.to;
            let v_rank = rank_map.entry(v.clone()).or_insert(0);

            // Mathematically push the target node down to accommodate the longest path
            if current_rank + 1 > *v_rank {
                *v_rank = current_rank + 1;
            }

            let deg = in_degrees.get_mut(v).unwrap();
            *deg -= 1;
            if *deg == 0 {
                queue.push(v.clone());
            }
        }
    }

    // Group by rank
    let mut max_rank = 0;
    for &r in rank_map.values() {
        if r > max_rank {
            max_rank = r;
        }
    }

    let mut ranks = vec![Vec::new(); max_rank + 1];
    for (node, rank) in rank_map {
        ranks[rank].push(node);
    }

    // Ensure edges span exactly ONE rank via dummy node insertion if necessary
    // (A complete Sugiyama implementation creates virtual geometry blocks here.
    // For scaffolding, we accept the ranking as is to proceed with logic.)

    RankedGraph {
        direction: graph.direction.clone(),
        ranks,
        nodes: graph.nodes().clone(),
        edges: graph.edges().to_vec(),
        subgraphs: graph.subgraphs.clone(),
    }
}
