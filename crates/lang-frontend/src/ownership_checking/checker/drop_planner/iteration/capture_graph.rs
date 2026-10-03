//! 以 lambda 身份构建有限捕获图，计算可达布局、环与并存 owned 根。
use std::collections::{BTreeMap, BTreeSet};

use super::super::DropPlanner;
use crate::{
    ast::ExpressionId,
    ownership_checking::{
        CleanupConditions, CleanupOwnerValueId, ClosureCaptureDescriptor, ClosureCaptureEffect,
        ClosureCaptureMode, ClosureCaptureSource, IterationCaptureGraph, IterationCaptureNode,
        IterationCaptureSource, IterationPhiCaptureSlot,
    },
};

/// Lambda 身份只分配一次；captured node index 可以回指已分配节点。
#[derive(Default)]
pub(super) struct PhiCaptureGraph {
    pub(super) nodes: Vec<PhiCaptureNode>,
    pub(super) by_closure: BTreeMap<usize, usize>,
}

pub(super) struct PhiCaptureNode {
    pub(super) closure: ExpressionId,
    pub(super) release_captures: Vec<ClosureCaptureDescriptor>,
    pub(super) sources: Vec<(ClosureCaptureDescriptor, usize, Vec<usize>)>,
    pub(super) opaque_sources: BTreeSet<ClosureCaptureSource>,
}

impl PhiCaptureGraph {
    pub(super) fn reachable(&self, roots: &[ExpressionId]) -> Vec<usize> {
        let mut seen = vec![false; self.nodes.len()];
        let mut pending = roots
            .iter()
            .filter_map(|root| self.by_closure.get(&root.index()).copied())
            .collect::<Vec<_>>();
        while let Some(node) = pending.pop() {
            if seen[node] {
                continue;
            }
            seen[node] = true;
            for (_, _, captured) in &self.nodes[node].sources {
                pending.extend(captured.iter().copied());
            }
        }
        seen.into_iter()
            .enumerate()
            .filter_map(|(node, reachable)| reachable.then_some(node))
            .collect()
    }

    pub(super) fn capture_layout(
        &self,
        roots: &[ExpressionId],
    ) -> Vec<(usize, ClosureCaptureSource, usize)> {
        self.reachable(roots)
            .into_iter()
            .flat_map(|node| {
                let entry = &self.nodes[node];
                entry
                    .sources
                    .iter()
                    .map(move |(capture, position, _)| (node, capture.source(), *position))
            })
            .collect()
    }

    pub(super) fn register_capture_layout(
        &self,
        roots: &[ExpressionId],
        owner: CleanupOwnerValueId,
        conditions: &mut CleanupConditions,
    ) -> Vec<IterationPhiCaptureSlot> {
        self.capture_layout(roots)
            .into_iter()
            .map(|(node, source, position)| {
                let slot = conditions.register_phi_capture_slot(
                    owner,
                    self.nodes[node].closure,
                    source,
                    position,
                );
                IterationPhiCaptureSlot {
                    node,
                    position,
                    slot,
                }
            })
            .collect()
    }

    pub(super) fn published(&self) -> IterationCaptureGraph {
        IterationCaptureGraph {
            nodes: self
                .nodes
                .iter()
                .map(|node| IterationCaptureNode {
                    closure: node.closure,
                    release_captures: node.release_captures.clone(),
                    sources: node
                        .sources
                        .iter()
                        .map(|(capture, position, captured)| IterationCaptureSource {
                            capture: *capture,
                            position: *position,
                            captured: captured.clone(),
                            may_be_opaque: node.opaque_sources.contains(&capture.source()),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    pub(super) fn insert(&mut self, closure: ExpressionId) -> usize {
        if let Some(&index) = self.by_closure.get(&closure.index()) {
            return index;
        }
        let index = self.nodes.len();
        self.by_closure.insert(closure.index(), index);
        self.nodes.push(PhiCaptureNode {
            closure,
            release_captures: Vec::new(),
            sources: Vec::new(),
            opaque_sources: BTreeSet::new(),
        });
        index
    }

    pub(super) fn expand(&mut self, planner: &DropPlanner<'_, '_>) {
        let mut index = 0;
        while index < self.nodes.len() {
            let closure = self.nodes[index].closure;
            let release_captures = planner.checker.captures_of(closure).collect::<Vec<_>>();
            let mut sources = Vec::new();
            let mut opaque_sources = BTreeSet::new();
            for (position, &capture) in release_captures.iter().enumerate() {
                if !planner.phi_capture_source_tracked(capture.source()) {
                    continue;
                }
                let captured = planner
                    .phi_capture_origins(closure, capture)
                    .iter()
                    .map(|&next| self.insert(next))
                    .collect();
                if capture.mode() == ClosureCaptureMode::Owned
                    && capture.effect() == ClosureCaptureEffect::Move
                    && planner
                        .captured_origins
                        .get(&(closure.index(), capture.source()))
                        .is_some_and(|origins| origins.may_be_opaque)
                {
                    opaque_sources.insert(capture.source());
                }
                sources.push((capture, position, captured));
            }
            self.nodes[index].release_captures = release_captures;
            self.nodes[index].sources = sources;
            self.nodes[index].opaque_sources = opaque_sources;
            index += 1;
        }
    }

    pub(super) fn recursive_origin(&self) -> Option<ExpressionId> {
        let edges = self.edges();
        cyclic_node(&edges).map(|index| self.nodes[index].closure)
    }

    /// 从叶端剥去无环路径；剩余节点恰是能沿 capture 边到达环的节点。
    pub(super) fn nodes_reaching_cycle(&self) -> Vec<bool> {
        nodes_reaching_cycle_in(&self.edges())
    }

    /// 仅根能沿 owned capture 边到达环时使用实例释放；独立叶根保留普通清理。
    pub(super) fn owned_nodes_reaching_cycle(&self) -> Vec<bool> {
        let edges = self
            .nodes
            .iter()
            .map(|node| {
                node.sources
                    .iter()
                    .filter(|(capture, _, _)| {
                        capture.mode() == ClosureCaptureMode::Owned
                            && capture.effect() == ClosureCaptureEffect::Move
                    })
                    .flat_map(|(_, _, captured)| captured.iter().copied())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        nodes_reaching_cycle_in(&edges)
    }

    /// Two slots of one formed parent may hold distinct instances of the same lambda.
    /// A diamond through alternative parents does not prove those parents coexist.
    pub(super) fn coexisting_owned_roots(
        &self,
        roots: &[usize],
        planner: &DropPlanner<'_, '_>,
    ) -> bool {
        let mut coexisting = false;
        for &root in roots {
            let mut seen = vec![false; self.nodes.len()];
            let mut pending = vec![root];
            while let Some(node) = pending.pop() {
                if seen[node] {
                    continue;
                }
                seen[node] = true;
                let mut actual_captures = 0;
                for capture in planner.checker.captures_of(self.nodes[node].closure) {
                    actual_captures += 1;
                    if capture.mode() != ClosureCaptureMode::Owned
                        || capture.effect() != ClosureCaptureEffect::Move
                    {
                        return false;
                    }
                }
                if actual_captures != self.nodes[node].sources.len() {
                    // The finite graph omits some captures; the root action cannot replace
                    // their ordinary drop or loan-end facts.
                    return false;
                }
                let mut prior_slots = BTreeSet::<usize>::new();
                for (_, _, children) in &self.nodes[node].sources {
                    if children.iter().any(|child| prior_slots.contains(child)) {
                        coexisting = true;
                    }
                    prior_slots.extend(children);
                    pending.extend(children);
                }
            }
        }
        coexisting
    }

    fn edges(&self) -> Vec<Vec<usize>> {
        self.nodes
            .iter()
            .map(|node| {
                node.sources
                    .iter()
                    .flat_map(|(_, _, captured)| captured.iter().copied())
                    .collect()
            })
            .collect()
    }

    /// 已知子环境与 opaque 值合流，或 tracked 子来源多选，均须读取父实例保存的选择。
    pub(super) fn conditional_nested_nodes(
        &self,
        captured_origins: &super::origins::CapturedOrigins,
    ) -> Vec<bool> {
        self.nodes
            .iter()
            .map(|node| {
                node.sources.iter().any(|(capture, _, captured)| {
                    let opaque = captured_origins
                        .get(&(node.closure.index(), capture.source()))
                        .is_some_and(|sources| sources.may_be_opaque);
                    (opaque && !captured.is_empty())
                        || (captured.len() > 1
                            && captured
                                .iter()
                                .any(|&child| !self.nodes[child].sources.is_empty()))
                })
            })
            .collect()
    }
}

fn nodes_reaching_cycle_in(edges: &[Vec<usize>]) -> Vec<bool> {
    let mut parents = vec![Vec::new(); edges.len()];
    let mut remaining = edges.iter().map(Vec::len).collect::<Vec<_>>();
    for (parent, children) in edges.iter().enumerate() {
        for &child in children {
            parents[child].push(parent);
        }
    }
    let mut pending = remaining
        .iter()
        .enumerate()
        .filter_map(|(node, &count)| (count == 0).then_some(node))
        .collect::<Vec<_>>();
    while let Some(node) = pending.pop() {
        for &parent in &parents[node] {
            remaining[parent] -= 1;
            if remaining[parent] == 0 {
                pending.push(parent);
            }
        }
    }
    remaining.into_iter().map(|count| count > 0).collect()
}

pub(super) fn cyclic_node(edges: &[Vec<usize>]) -> Option<usize> {
    cyclic_node_from_roots(edges, 0..edges.len())
}

fn cyclic_node_from_roots(
    edges: &[Vec<usize>],
    roots: impl IntoIterator<Item = usize>,
) -> Option<usize> {
    let mut marks = vec![0; edges.len()];
    for root in roots {
        if marks[root] != 0 {
            continue;
        }
        marks[root] = 1;
        let mut stack = vec![(root, 0)];
        while let Some((node, edge)) = stack.last_mut() {
            if *edge == edges[*node].len() {
                marks[*node] = 2;
                stack.pop();
                continue;
            }
            let next = edges[*node][*edge];
            *edge += 1;
            match marks[next] {
                0 => {
                    marks[next] = 1;
                    stack.push((next, 0));
                }
                1 => return Some(next),
                _ => {}
            }
        }
    }
    None
}

/// 工作表收集可达图节点，每个节点只记录一次（有限布局，不按捕获路径展开）。
/// 迭代实现：菱形阶梯不指数、深链不爆栈。
pub(super) fn finite_layout_order(edges: &[Vec<usize>], roots: &[usize]) -> Vec<usize> {
    let mut seen = vec![false; edges.len()];
    let mut order = Vec::new();
    let mut pending: Vec<usize> = roots.iter().rev().copied().collect();
    while let Some(node) = pending.pop() {
        if seen[node] {
            continue;
        }
        seen[node] = true;
        order.push(node);
        pending.extend(edges[node].iter().rev().copied());
    }
    order
}

impl DropPlanner<'_, '_> {
    fn phi_capture_source_tracked(&self, source: ClosureCaptureSource) -> bool {
        let ClosureCaptureSource::Symbol(symbol) = source else {
            return false;
        };
        self.checker.is_move_only_variable(symbol)
            && !self.checker.iterations.values().any(|plan| {
                plan.bindings()
                    .iter()
                    .any(|binding| binding.symbol() == symbol)
            })
    }

    fn phi_capture_origins(
        &self,
        closure: ExpressionId,
        capture: ClosureCaptureDescriptor,
    ) -> &[ExpressionId] {
        if capture.mode() != ClosureCaptureMode::Owned
            || capture.effect() != ClosureCaptureEffect::Move
        {
            return &[];
        }
        self.captured_origins
            .get(&(closure.index(), capture.source()))
            .map(|sources| sources.known.as_slice())
            .unwrap_or(&[])
    }
}
