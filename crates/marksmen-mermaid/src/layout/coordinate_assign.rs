//! Coordinate Assignment (Phase 3 of Sugiyama Framework)
//!
//! Transforms the topologically ranked and mathematically sorted graph structure
//! into an explicit absolute Cartesian geometry (X, Y canvas space vector paths).
//!
//! Currently implements a deterministic continuous layout strategy resembling Brandes-Köpf constraints.

use rustc_hash::FxHashMap;
use crate::layout::rank_assignment::RankedGraph;

#[derive(Debug, Clone)]
pub struct SpacedGraph {
    pub direction: String,
    pub nodes: FxHashMap<String, NodeGeometry>,
    pub edges: Vec<EdgeGeometry>,
    pub subgraphs: Vec<SubgraphGeometry>,
    /// Flowchart logical width in Typst points
    pub width: f64,
    /// Flowchart logical height in Typst points
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct NodeGeometry {
    pub label: String,
    pub shape: Option<crate::parsing::lexer::NodeShape>,
    pub style: crate::graph::directed_graph::StyleSpec,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct EdgeGeometry {
    pub path: Vec<(f64, f64)>, // Continuous spline coordinates
    pub label: Option<String>,
    pub style: crate::parsing::lexer::EdgeStyle,
}

#[derive(Debug, Clone)]
pub struct SubgraphGeometry {
    pub title: String,
    pub parent_title: Option<String>,
    pub depth: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Translates the logical ranked space into concrete graphic canvas space.
pub fn assign_coordinates(graph: &RankedGraph) -> SpacedGraph {
    let mut node_geometries: FxHashMap<String, NodeGeometry> = FxHashMap::default();
    let horizontal_flow = matches!(graph.direction.as_str(), "LR" | "RL");
    
    // Configurable spacings
    let level_spacing: f64 = 60.0;
    let node_spacing: f64 = 40.0;

    let mut primary_offset = 0.0;
    let mut max_cross_extent = 0.0;

    for rank in &graph.ranks {
        let mut ordered_rank = rank.clone();
        ordered_rank.sort_by(|left, right| {
            let left_path = node_subgraph_path(left, &graph.subgraphs);
            let right_path = node_subgraph_path(right, &graph.subgraphs);
            left_path.cmp(&right_path).then_with(|| left.cmp(right))
        });

        let mut cross_offset = 0.0;
        let mut max_primary_extent = 0.0;

        for node_id in &ordered_rank {
            if let Some(details) = graph.nodes.get(node_id) {
                // Ensure the node bounding box accommodates its label string length analytically
                let calculated_width = details.width.max((details.label.len() as f64 * 8.0) + 20.0);
                let calculated_height = details.height.max(30.0);
                let (x, y) = if horizontal_flow {
                    (primary_offset, cross_offset)
                } else {
                    (cross_offset, primary_offset)
                };

                node_geometries.insert(
                    node_id.clone(),
                    NodeGeometry {
                        label: details.label.clone(),
                        shape: details.shape.clone(),
                        style: details.style.clone(),
                        x,
                        y,
                        width: calculated_width,
                        height: calculated_height,
                    },
                );

                if horizontal_flow {
                    cross_offset += calculated_height + node_spacing;
                    if calculated_width > max_primary_extent {
                        max_primary_extent = calculated_width;
                    }
                } else {
                    cross_offset += calculated_width + node_spacing;
                    if calculated_height > max_primary_extent {
                        max_primary_extent = calculated_height;
                    }
                }
            }
        }

        if cross_offset > max_cross_extent {
            max_cross_extent = cross_offset;
        }
        primary_offset += max_primary_extent + level_spacing;
    }

    // Route edges as orthogonal polylines so they interact more cleanly with node boxes and
    // subgraph title bands than a single straight segment.
    let mut edge_geometries = Vec::new();
    for edge in &graph.edges {
        if let (Some(from_geom), Some(to_geom)) = (node_geometries.get(&edge.from), node_geometries.get(&edge.to)) {
            let path = if horizontal_flow {
                orthogonal_horizontal_path(from_geom, to_geom)
            } else {
                orthogonal_vertical_path(from_geom, to_geom)
            };

            edge_geometries.push(EdgeGeometry {
                path,
                label: edge.label.clone(),
                style: edge.style.clone(),
            });
        }
    }

    let mut subgraph_geometries = Vec::new();
    let mut known_bounds: FxHashMap<String, (f64, f64, f64, f64)> = FxHashMap::default();
    let mut ordered_subgraphs = graph.subgraphs.clone();
    ordered_subgraphs.sort_by(|left, right| right.depth.cmp(&left.depth).then_with(|| left.title.cmp(&right.title)));

    for subgraph in &ordered_subgraphs {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x: f64 = 0.0;
        let mut max_y: f64 = 0.0;
        let mut found = false;

        for node_id in &subgraph.nodes {
            if let Some(node) = node_geometries.get(node_id) {
                min_x = min_x.min(node.x);
                min_y = min_y.min(node.y);
                max_x = max_x.max(node.x + node.width);
                max_y = max_y.max(node.y + node.height);
                found = true;
            }
        }

        for child in graph.subgraphs.iter().filter(|candidate| candidate.parent_title.as_deref() == Some(subgraph.title.as_str())) {
            if let Some((child_min_x, child_min_y, child_max_x, child_max_y)) = known_bounds.get(&child.title) {
                min_x = min_x.min(*child_min_x);
                min_y = min_y.min(*child_min_y);
                max_x = max_x.max(*child_max_x);
                max_y = max_y.max(*child_max_y);
                found = true;
            }
        }

        if found {
            let frame_padding = (18.0 - (subgraph.depth as f64 * 2.0)).max(10.0);
            let top_padding = frame_padding + 12.0;
            let geometry = SubgraphGeometry {
                title: subgraph.title.clone(),
                parent_title: subgraph.parent_title.clone(),
                depth: subgraph.depth,
                x: (min_x - frame_padding).max(0.0),
                y: (min_y - top_padding).max(0.0),
                width: (max_x - min_x) + (frame_padding * 2.0),
                height: (max_y - min_y) + top_padding + frame_padding,
            };
            known_bounds.insert(
                geometry.title.clone(),
                (geometry.x, geometry.y, geometry.x + geometry.width, geometry.y + geometry.height),
            );
            subgraph_geometries.push(geometry);
        }
    }

    let subgraph_right = subgraph_geometries
        .iter()
        .map(|subgraph| subgraph.x + subgraph.width)
        .fold(0.0, f64::max);
    let subgraph_bottom = subgraph_geometries
        .iter()
        .map(|subgraph| subgraph.y + subgraph.height)
        .fold(0.0, f64::max);

    SpacedGraph {
        direction: graph.direction.clone(),
        nodes: node_geometries,
        edges: edge_geometries,
        subgraphs: subgraph_geometries,
        width: (if horizontal_flow { primary_offset } else { max_cross_extent }).max(subgraph_right + 24.0),
        height: (if horizontal_flow { max_cross_extent } else { primary_offset }).max(subgraph_bottom + 24.0),
    }
}

fn node_subgraph_path(node_id: &str, subgraphs: &[crate::graph::directed_graph::Subgraph]) -> Vec<usize> {
    let mut path: Vec<(usize, usize)> = subgraphs
        .iter()
        .enumerate()
        .filter(|(_, subgraph)| subgraph.nodes.iter().any(|node| node == node_id))
        .map(|(idx, subgraph)| (subgraph.depth, idx))
        .collect();
    path.sort();
    path.into_iter().map(|(_, idx)| idx).collect()
}

fn orthogonal_horizontal_path(from_geom: &NodeGeometry, to_geom: &NodeGeometry) -> Vec<(f64, f64)> {
    let start_x = from_geom.x + from_geom.width;
    let start_y = from_geom.y + (from_geom.height / 2.0);
    let end_x = to_geom.x;
    let end_y = to_geom.y + (to_geom.height / 2.0);

    if (start_y - end_y).abs() < 6.0 {
        return vec![(start_x, start_y), (end_x, end_y)];
    }

    let elbow_offset = ((end_x - start_x).abs() * 0.5).max(18.0);
    let mid_x = if end_x >= start_x {
        start_x + elbow_offset
    } else {
        start_x - elbow_offset
    };

    vec![
        (start_x, start_y),
        (mid_x, start_y),
        (mid_x, end_y),
        (end_x, end_y),
    ]
}

fn orthogonal_vertical_path(from_geom: &NodeGeometry, to_geom: &NodeGeometry) -> Vec<(f64, f64)> {
    let start_x = from_geom.x + (from_geom.width / 2.0);
    let start_y = from_geom.y + from_geom.height;
    let end_x = to_geom.x + (to_geom.width / 2.0);
    let end_y = to_geom.y;

    if (start_x - end_x).abs() < 6.0 {
        return vec![(start_x, start_y), (end_x, end_y)];
    }

    let elbow_offset = ((end_y - start_y).abs() * 0.5).max(18.0);
    let mid_y = if end_y >= start_y {
        start_y + elbow_offset
    } else {
        start_y - elbow_offset
    };

    vec![
        (start_x, start_y),
        (start_x, mid_y),
        (end_x, mid_y),
        (end_x, end_y),
    ]
}
