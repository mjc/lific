use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::db::models::{Issue, ProjectRelation, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Canvas {
    Linked,
    Unlinked,
}

#[derive(Debug, Clone)]
pub(crate) struct GraphProjection {
    pub(crate) canvas: Canvas,
    pub(crate) issues: Vec<Issue>,
    pub(crate) relations: Vec<ProjectRelation>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Point {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GraphLayout {
    pub(crate) positions: BTreeMap<i64, Point>,
    pub(crate) component_by_issue: BTreeMap<i64, usize>,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RelationKind {
    Blocks,
    RelatesTo,
    Duplicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RelationEdge {
    pub(crate) source: i64,
    pub(crate) target: i64,
    pub(crate) kind: RelationKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LayoutOptions {
    pub(crate) node_width: f64,
    pub(crate) node_height: f64,
    pub(crate) gap_x: f64,
    pub(crate) gap_y: f64,
    pub(crate) component_gap: f64,
}

pub(crate) const DAG_OPTIONS: LayoutOptions = LayoutOptions {
    node_width: 200.0,
    node_height: 58.0,
    gap_x: 90.0,
    gap_y: 18.0,
    component_gap: 48.0,
};

pub(crate) const GRID_OPTIONS: LayoutOptions = LayoutOptions {
    node_width: 200.0,
    node_height: 58.0,
    gap_x: 24.0,
    gap_y: 16.0,
    component_gap: 0.0,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GraphCounts {
    pub(crate) linked: usize,
    pub(crate) unlinked: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct FilteredGraph {
    pub(crate) linked: GraphProjection,
    pub(crate) unlinked: GraphProjection,
    pub(crate) counts: GraphCounts,
}

pub(crate) fn project(
    issues: &[Issue],
    relations: &[ProjectRelation],
    show_closed: bool,
) -> FilteredGraph {
    let visible_issues: Vec<_> = issues
        .iter()
        .filter(|issue| show_closed || is_open(issue.status))
        .cloned()
        .collect();
    let visible_ids: BTreeSet<_> = visible_issues.iter().map(|issue| issue.id).collect();
    let visible_relations: Vec<_> = relations
        .iter()
        .filter(|relation| {
            visible_ids.contains(&relation.source_id) && visible_ids.contains(&relation.target_id)
        })
        .cloned()
        .collect();
    let linked_ids: BTreeSet<_> = visible_relations
        .iter()
        .flat_map(|relation| [relation.source_id, relation.target_id])
        .collect();
    let linked_issues: Vec<_> = visible_issues
        .iter()
        .filter(|issue| linked_ids.contains(&issue.id))
        .cloned()
        .collect();
    let unlinked_issues: Vec<_> = visible_issues
        .iter()
        .filter(|issue| !linked_ids.contains(&issue.id))
        .cloned()
        .collect();
    let linked_relations = visible_relations
        .into_iter()
        .filter(|relation| linked_ids.contains(&relation.source_id))
        .collect();
    let counts = GraphCounts {
        linked: linked_issues.len(),
        unlinked: unlinked_issues.len(),
    };
    FilteredGraph {
        linked: GraphProjection {
            canvas: Canvas::Linked,
            issues: linked_issues,
            relations: linked_relations,
        },
        unlinked: GraphProjection {
            canvas: Canvas::Unlinked,
            issues: unlinked_issues,
            relations: Vec::new(),
        },
        counts,
    }
}

fn is_open(status: Status) -> bool {
    matches!(status, Status::Backlog | Status::Todo | Status::Active)
}

pub(crate) fn layout_linked(
    issue_ids: &[i64],
    blocking_edges: &[RelationEdge],
    cluster_edges: &[RelationEdge],
) -> GraphLayout {
    if issue_ids.is_empty() {
        return empty_layout();
    }
    let mut components = components(issue_ids, cluster_edges);
    components.sort_by_key(|component| std::cmp::Reverse(component.len()));
    let mut component_positions = Vec::new();
    let mut width = 0.0_f64;
    let mut height = 0.0_f64;
    for component in &components {
        let members: BTreeSet<_> = component.iter().copied().collect();
        let edges: Vec<_> = blocking_edges
            .iter()
            .copied()
            .filter(|edge| members.contains(&edge.source) && members.contains(&edge.target))
            .collect();
        let layout = layout_component(component, &edges);
        width = width.max(layout.width);
        component_positions.push((layout.positions, height));
        height += layout.height + DAG_OPTIONS.component_gap;
    }
    height -= DAG_OPTIONS.component_gap;
    let mut positions = BTreeMap::new();
    let mut component_by_issue = BTreeMap::new();
    for (component_index, (component, (component_nodes, offset_y))) in
        components.iter().zip(component_positions).enumerate()
    {
        for id in component {
            component_by_issue.insert(*id, component_index);
        }
        for (id, mut point) in component_nodes {
            point.y += offset_y;
            positions.insert(id, point);
        }
    }
    GraphLayout {
        positions,
        component_by_issue,
        width,
        height,
    }
}

fn components(node_ids: &[i64], edges: &[RelationEdge]) -> Vec<Vec<i64>> {
    let mut neighbors: BTreeMap<i64, Vec<i64>> =
        node_ids.iter().map(|id| (*id, Vec::new())).collect();
    for edge in edges {
        if let Some(list) = neighbors.get_mut(&edge.source) {
            list.push(edge.target);
        }
        if let Some(list) = neighbors.get_mut(&edge.target) {
            list.push(edge.source);
        }
    }
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for start in node_ids {
        if seen.contains(start) {
            continue;
        }
        let mut stack = vec![*start];
        seen.insert(*start);
        let mut component = Vec::new();
        while let Some(node) = stack.pop() {
            component.push(node);
            for next in neighbors.get(&node).into_iter().flatten() {
                if seen.insert(*next) {
                    stack.push(*next);
                }
            }
        }
        result.push(component);
    }
    result
}

fn layout_component(nodes: &[i64], edges: &[RelationEdge]) -> GraphLayout {
    let back_edges = find_back_edges(nodes, edges);
    let acyclic: Vec<_> = edges
        .iter()
        .filter(|edge| !back_edges.contains(edge))
        .copied()
        .collect();
    let layer_of = assign_layers(nodes, &acyclic);
    let ordered_layers = order_layers(nodes, &layer_of, &acyclic);
    let layer_indices: Vec<_> = ordered_layers.keys().copied().collect();
    let tallest = layer_indices
        .iter()
        .map(|layer| ordered_layers[layer].len())
        .max()
        .unwrap_or(0);
    let height = if tallest == 0 {
        0.0
    } else {
        tallest as f64 * DAG_OPTIONS.node_height
            + tallest.saturating_sub(1) as f64 * DAG_OPTIONS.gap_y
    };
    let mut width = 0.0_f64;
    let mut positions = BTreeMap::new();
    for layer in layer_indices {
        let row = &ordered_layers[&layer];
        let row_height = row.len() as f64 * DAG_OPTIONS.node_height
            + row.len().saturating_sub(1) as f64 * DAG_OPTIONS.gap_y;
        let y_offset = (height - row_height) / 2.0;
        for (index, node) in row.iter().enumerate() {
            let x = layer as f64 * (DAG_OPTIONS.node_width + DAG_OPTIONS.gap_x);
            let y = y_offset + index as f64 * (DAG_OPTIONS.node_height + DAG_OPTIONS.gap_y);
            positions.insert(*node, Point { x, y });
            width = width.max(x + DAG_OPTIONS.node_width);
        }
    }
    GraphLayout {
        positions,
        component_by_issue: BTreeMap::new(),
        width,
        height,
    }
}

fn find_back_edges(nodes: &[i64], edges: &[RelationEdge]) -> BTreeSet<RelationEdge> {
    let mut outgoing: BTreeMap<i64, Vec<RelationEdge>> =
        nodes.iter().map(|node| (*node, Vec::new())).collect();
    for edge in edges {
        if let Some(list) = outgoing.get_mut(&edge.source) {
            list.push(*edge);
        }
    }
    let mut colors: BTreeMap<i64, u8> = nodes.iter().map(|node| (*node, 0)).collect();
    let mut back = BTreeSet::new();
    for root in nodes {
        if colors.get(root) != Some(&0) {
            continue;
        }
        let mut stack = vec![(*root, 0_usize)];
        colors.insert(*root, 1);
        while let Some((node, edge_index)) = stack.last().copied() {
            let outgoing_edges = outgoing.get(&node).map_or(&[][..], Vec::as_slice);
            if edge_index >= outgoing_edges.len() {
                colors.insert(node, 2);
                stack.pop();
                continue;
            }
            let edge = outgoing_edges[edge_index];
            if let Some(frame) = stack.last_mut() {
                frame.1 += 1;
            }
            match colors.get(&edge.target).copied().unwrap_or(2) {
                0 => {
                    colors.insert(edge.target, 1);
                    stack.push((edge.target, 0));
                }
                1 => {
                    back.insert(edge);
                }
                _ => {}
            }
        }
    }
    back
}

fn assign_layers(nodes: &[i64], edges: &[RelationEdge]) -> BTreeMap<i64, usize> {
    let mut successors: BTreeMap<i64, Vec<i64>> =
        nodes.iter().map(|node| (*node, Vec::new())).collect();
    let mut indegree: BTreeMap<i64, usize> = nodes.iter().map(|node| (*node, 0)).collect();
    for edge in edges {
        successors.entry(edge.source).or_default().push(edge.target);
        *indegree.entry(edge.target).or_default() += 1;
    }
    let mut layers: BTreeMap<i64, usize> = nodes.iter().map(|node| (*node, 0)).collect();
    let mut queue: VecDeque<_> = nodes
        .iter()
        .filter(|node| indegree.get(node) == Some(&0))
        .copied()
        .collect();
    while let Some(node) = queue.pop_front() {
        for next in successors.get(&node).into_iter().flatten() {
            let next_layer = layers.get(&node).copied().unwrap_or(0) + 1;
            layers
                .entry(*next)
                .and_modify(|layer| *layer = (*layer).max(next_layer));
            let remaining = indegree.entry(*next).or_default();
            *remaining -= 1;
            if *remaining == 0 {
                queue.push_back(*next);
            }
        }
    }
    layers
}

fn order_layers(
    nodes: &[i64],
    layer_of: &BTreeMap<i64, usize>,
    edges: &[RelationEdge],
) -> BTreeMap<usize, Vec<i64>> {
    let mut layers: BTreeMap<usize, Vec<i64>> = BTreeMap::new();
    for node in nodes {
        let layer = layer_of.get(node).copied().unwrap_or(0);
        layers.entry(layer).or_default().push(*node);
    }
    let layer_indices: Vec<_> = layers.keys().copied().collect();
    let mut positions = BTreeMap::new();
    reindex(&layers, &mut positions);
    let mut predecessors: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut successors: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for edge in edges {
        successors.entry(edge.source).or_default().push(edge.target);
        predecessors
            .entry(edge.target)
            .or_default()
            .push(edge.source);
    }
    for _ in 0..4 {
        sweep_layers(&mut layers, &layer_indices, &predecessors, &mut positions);
        let reversed: Vec<_> = layer_indices.iter().rev().copied().collect();
        sweep_layers(&mut layers, &reversed, &successors, &mut positions);
    }
    layers
}

fn reindex(layers: &BTreeMap<usize, Vec<i64>>, positions: &mut BTreeMap<i64, usize>) {
    for row in layers.values() {
        for (index, node) in row.iter().enumerate() {
            positions.insert(*node, index);
        }
    }
}

fn sweep_layers(
    layers: &mut BTreeMap<usize, Vec<i64>>,
    order: &[usize],
    neighbors_of: &BTreeMap<i64, Vec<i64>>,
    positions: &mut BTreeMap<i64, usize>,
) {
    for layer in order {
        let Some(row) = layers.get_mut(layer) else {
            continue;
        };
        let barycenters: BTreeMap<_, _> = row
            .iter()
            .map(|node| {
                let neighbors = neighbors_of.get(node).map_or(&[][..], Vec::as_slice);
                let barycenter = if neighbors.is_empty() {
                    positions.get(node).copied().unwrap_or(0) as f64
                } else {
                    neighbors
                        .iter()
                        .map(|neighbor| positions.get(neighbor).copied().unwrap_or(0) as f64)
                        .sum::<f64>()
                        / neighbors.len() as f64
                };
                (*node, barycenter)
            })
            .collect();
        row.sort_by(|left, right| barycenters[left].total_cmp(&barycenters[right]));
        for (index, node) in row.iter().enumerate() {
            positions.insert(*node, index);
        }
    }
}

fn empty_layout() -> GraphLayout {
    GraphLayout {
        positions: BTreeMap::new(),
        component_by_issue: BTreeMap::new(),
        width: 0.0,
        height: 0.0,
    }
}

pub(crate) fn layout_unlinked(issue_ids: &[i64]) -> GraphLayout {
    if issue_ids.is_empty() {
        return empty_layout();
    }
    #[expect(
        clippy::cast_sign_loss,
        reason = "The square root of a nonnegative node count cannot be negative."
    )]
    let column_count = ((issue_ids.len() as f64 * 1.6).sqrt().ceil() as usize).max(1);
    let row_step = GRID_OPTIONS.node_height + GRID_OPTIONS.gap_y;
    let column_step = GRID_OPTIONS.node_width + GRID_OPTIONS.gap_x;
    let mut positions = BTreeMap::new();
    for (index, id) in issue_ids.iter().enumerate() {
        positions.insert(
            *id,
            Point {
                x: (index % column_count) as f64 * column_step,
                y: (index / column_count) as f64 * row_step,
            },
        );
    }
    GraphLayout {
        positions,
        component_by_issue: BTreeMap::new(),
        width: issue_ids.len().min(column_count) as f64 * column_step - GRID_OPTIONS.gap_x,
        height: issue_ids.len().div_ceil(column_count) as f64 * row_step - GRID_OPTIONS.gap_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{Priority, Status};

    fn issue(id: i64, status: Status) -> Issue {
        Issue {
            id,
            project_id: 7,
            sequence: id,
            identifier: format!("G-{id}"),
            title: format!("Issue {id}"),
            description: String::new(),
            status,
            priority: Priority::None,
            module_id: None,
            sort_order: id as f64,
            start_date: None,
            target_date: None,
            created_at: String::new(),
            updated_at: String::new(),
            seq: 0,
            source: None,
            labels: Vec::new(),
            blocks: Vec::new(),
            blocked_by: Vec::new(),
            relates_to: Vec::new(),
            duplicates: Vec::new(),
            duplicated_by: Vec::new(),
            waits: Vec::new(),
        }
    }

    fn relation(source: i64, target: i64, relation_type: &str) -> ProjectRelation {
        ProjectRelation {
            source_id: source,
            source_identifier: format!("G-{source}"),
            target_id: target,
            target_identifier: format!("G-{target}"),
            relation_type: relation_type.into(),
        }
    }

    #[test]
    fn projection_filters_closed_and_partitions_on_visible_endpoints() {
        let issues = [
            issue(1, Status::Todo),
            issue(2, Status::Active),
            issue(3, Status::Done),
            issue(4, Status::Backlog),
        ];
        let relations = [relation(1, 2, "blocks"), relation(2, 3, "relates_to")];

        let open = project(&issues, &relations, false);
        assert_eq!(
            open.linked.issues.iter().map(|i| i.id).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(open.linked.relations.len(), 1);
        assert_eq!(
            open.unlinked
                .issues
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>(),
            [4]
        );
        assert_eq!(
            open.counts,
            GraphCounts {
                linked: 2,
                unlinked: 1
            }
        );

        let closed = project(&issues, &relations, true);
        assert_eq!(
            closed
                .linked
                .issues
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(closed.linked.relations.len(), 2);
        assert_eq!(
            closed
                .unlinked
                .issues
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>(),
            [4]
        );
    }

    #[test]
    fn grid_layout_preserves_input_order_and_geometry() {
        let layout = layout_unlinked(&[9, 2, 5]);
        assert_eq!(layout.positions.get(&9), Some(&Point { x: 0.0, y: 0.0 }));
        assert_eq!(layout.positions.get(&2), Some(&Point { x: 224.0, y: 0.0 }));
        assert_eq!(layout.positions.get(&5), Some(&Point { x: 448.0, y: 0.0 }));
    }

    #[test]
    fn blocker_layout_keeps_cycles_and_non_blocking_relations_in_one_component() {
        let relations = [
            RelationEdge {
                source: 1,
                target: 2,
                kind: RelationKind::Blocks,
            },
            RelationEdge {
                source: 2,
                target: 1,
                kind: RelationKind::Blocks,
            },
            RelationEdge {
                source: 2,
                target: 3,
                kind: RelationKind::RelatesTo,
            },
            RelationEdge {
                source: 4,
                target: 5,
                kind: RelationKind::Duplicate,
            },
        ];
        let blockers: Vec<_> = relations
            .iter()
            .copied()
            .filter(|edge| edge.kind == RelationKind::Blocks)
            .collect();
        let layout = layout_linked(&[1, 2, 3, 4, 5], &blockers, &relations);
        assert_eq!(layout.positions.len(), 5);
        assert_eq!(layout.component_by_issue[&1], layout.component_by_issue[&2]);
        assert_eq!(layout.component_by_issue[&1], layout.component_by_issue[&3]);
        assert_eq!(layout.component_by_issue[&4], layout.component_by_issue[&5]);
        assert_ne!(layout.component_by_issue[&1], layout.component_by_issue[&4]);
    }

    fn point(layout: &GraphLayout, id: i64) -> Point {
        layout.positions[&id]
    }

    #[test]
    fn fork_layout_matches_main_dfs_order_and_centering() {
        let edges = [
            RelationEdge {
                source: 1,
                target: 2,
                kind: RelationKind::Blocks,
            },
            RelationEdge {
                source: 1,
                target: 3,
                kind: RelationKind::Blocks,
            },
        ];
        let layout = layout_linked(&[1, 2, 3], &edges, &edges);
        assert_eq!(point(&layout, 1), Point { x: 0.0, y: 38.0 });
        assert_eq!(point(&layout, 2), Point { x: 290.0, y: 76.0 });
        assert_eq!(point(&layout, 3), Point { x: 290.0, y: 0.0 });
    }

    #[test]
    fn long_edges_across_layers_match_main_barycenter_sweeps() {
        let edges = [
            RelationEdge {
                source: 1,
                target: 2,
                kind: RelationKind::Blocks,
            },
            RelationEdge {
                source: 1,
                target: 5,
                kind: RelationKind::Blocks,
            },
            RelationEdge {
                source: 2,
                target: 4,
                kind: RelationKind::Blocks,
            },
            RelationEdge {
                source: 3,
                target: 4,
                kind: RelationKind::Blocks,
            },
        ];
        let layout = layout_linked(&[1, 2, 3, 4, 5], &edges, &edges);
        assert_eq!(point(&layout, 3), Point { x: 0.0, y: 0.0 });
        assert_eq!(point(&layout, 1), Point { x: 0.0, y: 76.0 });
        assert_eq!(point(&layout, 5), Point { x: 290.0, y: 0.0 });
        assert_eq!(point(&layout, 2), Point { x: 290.0, y: 76.0 });
        assert_eq!(point(&layout, 4), Point { x: 580.0, y: 38.0 });
    }

    #[test]
    fn relation_only_clustering_and_no_neighbor_fallback_match_main() {
        let blockers = [RelationEdge {
            source: 2,
            target: 3,
            kind: RelationKind::Blocks,
        }];
        let clusters = [
            RelationEdge {
                source: 2,
                target: 3,
                kind: RelationKind::Blocks,
            },
            RelationEdge {
                source: 1,
                target: 2,
                kind: RelationKind::RelatesTo,
            },
            RelationEdge {
                source: 1,
                target: 3,
                kind: RelationKind::RelatesTo,
            },
        ];
        let layout = layout_linked(&[1, 2, 3], &blockers, &clusters);
        assert_eq!(point(&layout, 1), Point { x: 0.0, y: 0.0 });
        assert_eq!(point(&layout, 2), Point { x: 0.0, y: 76.0 });
        assert_eq!(point(&layout, 3), Point { x: 290.0, y: 38.0 });
    }
}
