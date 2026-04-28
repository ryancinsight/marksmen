//! Crossing Reduction (Phase 2 of Sugiyama Framework)
//!
//! Reorders nodes horizontally within their defined ranks to minimize edge crossovers
//! between adjacent layers. Utilizes the barycenter heuristic.

use crate::layout::rank_assignment::RankedGraph;
use rustc_hash::FxHashMap;

/// Iteratively sorts nodes within each rank based on the average position (barycenter)
/// of their neighbors in the adjacent rank to heuristically minimize crossing edges.
pub fn minimize_crossings(graph: &mut RankedGraph) {
    let max_iterations = 4; // Empirical bound for stable deterministic layouts

    for _ in 0..max_iterations {
        // Downward sweep (Rank 0 -> N)
        for r in 1..graph.ranks.len() {
            let mut barycenters: FxHashMap<String, f64> = FxHashMap::default();

            for node in &graph.ranks[r] {
                let upstream_neighbors: Vec<String> = graph
                    .edges
                    .iter()
                    .filter(|e| e.to == *node && graph.ranks[r - 1].contains(&e.from))
                    .map(|e| e.from.clone())
                    .collect();

                if upstream_neighbors.is_empty() {
                    barycenters.insert(node.clone(), (graph.ranks[r].len() as f64) / 2.0);
                } else {
                    let mut sum = 0.0;
                    for un in &upstream_neighbors {
                        if let Some(pos) = graph.ranks[r - 1].iter().position(|x| x == un) {
                            sum += pos as f64;
                        }
                    }
                    barycenters.insert(node.clone(), sum / (upstream_neighbors.len() as f64));
                }
            }

            // Sort rank[r] by calculated barycenter
            graph.ranks[r].sort_by(|a, b| {
                let ba = barycenters.get(a).unwrap();
                let bb = barycenters.get(b).unwrap();
                ba.partial_cmp(bb).unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        // Upward sweep (N -> 0)
        for r in (0..graph.ranks.len() - 1).rev() {
            let mut barycenters: FxHashMap<String, f64> = FxHashMap::default();

            for node in &graph.ranks[r] {
                let downstream_neighbors: Vec<String> = graph
                    .edges
                    .iter()
                    .filter(|e| e.from == *node && graph.ranks[r + 1].contains(&e.to))
                    .map(|e| e.to.clone())
                    .collect();

                if downstream_neighbors.is_empty() {
                    barycenters.insert(node.clone(), (graph.ranks[r].len() as f64) / 2.0);
                } else {
                    let mut sum = 0.0;
                    for dn in &downstream_neighbors {
                        if let Some(pos) = graph.ranks[r + 1].iter().position(|x| x == dn) {
                            sum += pos as f64;
                        }
                    }
                    barycenters.insert(node.clone(), sum / (downstream_neighbors.len() as f64));
                }
            }

            // Sort rank[r] by calculated barycenter
            graph.ranks[r].sort_by(|a, b| {
                let ba = barycenters.get(a).unwrap();
                let bb = barycenters.get(b).unwrap();
                ba.partial_cmp(bb).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }
}
