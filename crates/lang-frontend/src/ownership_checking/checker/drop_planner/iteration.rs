//! Provider 使用期间保留 source owner；提前退出按内层到外层释放临时容器。
use std::collections::{BTreeMap, BTreeSet};

use super::{
    ClosureOrigin, DropFact, DropPlanner, DropPoint, DropTarget, ExpressionUse,
    IterationCleanupAction as Action, IterationExitKind, IterationExitPlan, IterationOwnershipPlan,
    OwnedValue, OwnerVersion, OwnershipCheckingError, RetainedSource, ValueState,
};
use crate::ownership_checking::{
    CleanupCaptureInput, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
    CleanupConditions, CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorId,
    ClosureCaptureDescriptor, ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource,
    IterationCaptureGraph, IterationCaptureNode, IterationCaptureSource,
    IterationClosurePhiBinding, IterationClosurePhiOrigin, IterationClosurePhiSource,
    IterationPhiBoundary, IterationPhiCaptureSlot, IterationPhiIncoming,
    IterationPhiIncomingBinding, IterationPhiIncomingEnvironment, IterationPhiIncomingKind,
    IterationPhiIncomingOrigin, IterationPhiIncomingSource, IterationPhiIncomingValue,
    IterationPhiPresenceSource, IterationPhiRootSource, IterationPhiSelectorWrite,
};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    source::Span,
};

#[derive(Clone, Debug)]
pub(super) struct IterationFrame {
    statement: StatementId,
    element_active: bool,
    pub(super) source_root: Option<SymbolId>,
    temporary: Option<(ExpressionId, Span, CleanupOwnerValueId)>,
    pub(super) scope_depth: usize,
    pub(super) loop_depth: usize,
}

#[derive(Clone, Copy)]
struct PhiAllocation {
    statement: StatementId,
    boundary: IterationPhiBoundary,
    origin: Span,
}

/// Lambda 身份只分配一次；captured node index 可以回指已分配节点。
#[derive(Default)]
struct PhiCaptureGraph {
    nodes: Vec<PhiCaptureNode>,
    by_closure: BTreeMap<usize, usize>,
}

struct PhiCaptureNode {
    closure: ExpressionId,
    release_captures: Vec<ClosureCaptureDescriptor>,
    sources: Vec<(ClosureCaptureDescriptor, usize, Vec<usize>)>,
    opaque_sources: BTreeSet<ClosureCaptureSource>,
}

impl PhiCaptureGraph {
    fn reachable(&self, roots: &[ExpressionId]) -> Vec<usize> {
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

    fn capture_layout(&self, roots: &[ExpressionId]) -> Vec<(usize, ClosureCaptureSource, usize)> {
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

    fn register_capture_layout(
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

    fn published(&self) -> IterationCaptureGraph {
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

    fn insert(&mut self, closure: ExpressionId) -> usize {
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

    fn expand(&mut self, planner: &DropPlanner<'_, '_>) {
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

    fn recursive_origin(&self) -> Option<ExpressionId> {
        let edges = self.edges();
        cyclic_node(&edges).map(|index| self.nodes[index].closure)
    }

    /// 从叶端剥去无环路径；剩余节点恰是能沿 capture 边到达环的节点。
    fn nodes_reaching_cycle(&self) -> Vec<bool> {
        nodes_reaching_cycle_in(&self.edges())
    }

    /// 仅根能沿 owned capture 边到达环时使用实例释放；独立叶根保留普通清理。
    fn owned_nodes_reaching_cycle(&self) -> Vec<bool> {
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
    fn coexisting_owned_roots(&self, roots: &[usize], planner: &DropPlanner<'_, '_>) -> bool {
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
    fn conditional_nested_nodes(
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

fn cyclic_node(edges: &[Vec<usize>]) -> Option<usize> {
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

struct PhiSeedContext<'a, 'b, 'checker> {
    checker: &'a super::Checker<'checker>,
    current_environment: Option<(CleanupOwnerValueId, ExpressionId)>,
    selected: &'b [&'b IterationClosurePhiBinding],
    statement: StatementId,
}

struct ForwardPhiContext<'a, 'checker> {
    conditions: &'a mut CleanupConditions,
    checker: &'a super::super::Checker<'checker>,
}

#[allow(clippy::too_many_arguments)]
fn record_phi_origin(
    conditions: &mut CleanupConditions,
    root: CleanupOwnerValueId,
    layout: &IterationClosurePhiOrigin,
    candidates: &[&ClosureOrigin],
    available_when: CleanupConditionId,
    instance_path: Option<(CleanupOwnerValueId, Vec<usize>)>,
    origin_layouts: &[IterationClosurePhiOrigin],
    enclosing_capture_phi: &mut Option<ExpressionId>,
) -> IterationPhiIncomingOrigin {
    let mut condition = CleanupConditionId::NEVER;
    let mut environments = Vec::new();
    for candidate in candidates
        .iter()
        .copied()
        .filter(|candidate| candidate.closure == layout.closure())
    {
        let (instance_root, capture_path) = instance_path
            .clone()
            .unwrap_or_else(|| (candidate.owner, Vec::new()));
        let selected = conditions.and(candidate.condition, available_when);
        if selected == CleanupConditionId::NEVER {
            continue;
        }
        condition = conditions.or(condition, selected);
        let mut sources = Vec::new();
        for input in &candidate.inputs {
            let input_condition = conditions.and(selected, input.condition);
            if input_condition == CleanupConditionId::NEVER {
                continue;
            }
            let slot = layout
                .sources()
                .iter()
                .find(|slot| slot.source() == input.source);
            let target = slot.and_then(|slot| {
                (!matches!(input.value, CleanupCaptureValue::Place(_))).then_some(slot.owner())
            });
            let capture_slot = slot.map(|_| {
                conditions
                    .phi_capture_slot(root, layout.closure(), input.source)
                    .expect("preallocated phi capture has a registered layout slot")
            });
            let source_capture_slot = conditions
                .capture_slot(candidate.layout_owner, input.source)
                .or_else(|| {
                    conditions.phi_capture_slot(
                        candidate.layout_owner,
                        candidate.closure,
                        input.source,
                    )
                });
            let read_address = source_capture_slot
                .map(|_| conditions.register_instance_address(instance_root, &capture_path));
            let owned = match input.value {
                CleanupCaptureValue::Owner(owner) => Some(owner),
                _ => None,
            };
            let owned_descendant = slot.is_some_and(|slot| {
                let nested = slot.captured();
                nested
                    .iter()
                    .any(|nested| !origin_layouts[*nested].sources().is_empty())
                    && !(nested.len() == 1 && descendant_expandable(&origin_layouts[nested[0]]))
            });
            let sibling_alternatives = candidate
                .captured
                .iter()
                .filter(|origin| origin.captured_from == Some(input.source))
                .take(2)
                .count()
                > 1;
            let readable_from_parent = sibling_alternatives
                && mutually_exclusive(conditions, &candidate.captured)
                && (conditions
                    .owner_snapshot(candidate.owner)
                    .is_some_and(|snapshot| {
                        snapshot
                            .copies()
                            .iter()
                            .any(|copy| copy.source_value().is_some())
                    })
                    || matches!(
                        conditions.owner_value(candidate.owner),
                        Some(CleanupOwnerValue::IterationPhi { .. })
                    ));
            if matches!(input.value, CleanupCaptureValue::Environment { .. })
                && (owned_descendant || (sibling_alternatives && !readable_from_parent))
            {
                // 内层形成动作若仍读外层形成前的条件，会在外部选择改变后选错子环境。
                enclosing_capture_phi.get_or_insert(layout.closure());
            }
            let nested_path = capture_slot.map(|slot| {
                let mut path = capture_path.clone();
                path.push(
                    conditions
                        .capture_slot_value(slot)
                        .expect("preallocated phi capture has a registered position")
                        .position(),
                );
                (instance_root, path)
            });
            let captured = slot
                .into_iter()
                .flat_map(|slot| {
                    slot.captured()
                        .iter()
                        .map(move |index| &origin_layouts[*index])
                })
                .map(|nested| {
                    let nested_candidates = candidate
                        .captured
                        .iter()
                        .filter(|origin| {
                            origin.captured_from == Some(input.source)
                                && owned.is_none_or(|owner| origin.owner == owner)
                        })
                        .collect::<Vec<_>>();
                    record_phi_origin(
                        conditions,
                        root,
                        nested,
                        &nested_candidates,
                        input_condition,
                        nested_path.clone(),
                        origin_layouts,
                        enclosing_capture_phi,
                    )
                })
                .collect();
            sources.push(IterationPhiIncomingSource {
                target,
                capture_slot,
                source_capture_slot,
                read_address,
                source_environment: candidate.owner,
                nested_instance: !capture_path.is_empty(),
                input: CleanupCaptureInput {
                    condition: input_condition,
                    ..*input
                },
                captured,
            });
        }
        environments.push(IterationPhiIncomingEnvironment {
            owner: candidate.owner,
            instance_root,
            capture_path,
            condition: selected,
            sources,
        });
    }
    IterationPhiIncomingOrigin {
        node: layout.node(),
        target: layout.selector(),
        condition,
        environments,
    }
}

/// selector 写集按完整有限布局补齐 false，再合并各路径的局部条件。
/// 不修改 origin/environment，避免把一条父路径的子环境搬到另一条路径。
fn phi_selector_writes(
    conditions: &mut CleanupConditions,
    layout: &[IterationClosurePhiOrigin],
    origins: &[IterationPhiIncomingOrigin],
    root_sources: &[IterationPhiRootSource],
) -> Vec<IterationPhiSelectorWrite> {
    let mut conditions_by_selector = layout
        .iter()
        .map(|node| (node.selector(), CleanupConditionId::NEVER))
        .collect::<BTreeMap<CleanupSelectorId, CleanupConditionId>>();
    let mut pending = origins.iter().collect::<Vec<_>>();
    while let Some(origin) = pending.pop() {
        let condition = conditions_by_selector
            .get_mut(&origin.target())
            .expect("incoming origin belongs to the allocated phi layout");
        *condition = conditions.or(*condition, origin.condition());
        for environment in origin.environments() {
            for source in environment.sources() {
                pending.extend(source.captured());
            }
        }
    }
    if origins.is_empty() {
        for source in root_sources {
            let selector = layout
                .iter()
                .find(|node| node.node() == source.node())
                .expect("root source belongs to the allocated phi layout")
                .selector();
            let condition = conditions_by_selector.get_mut(&selector).unwrap();
            *condition = conditions.or(*condition, source.condition());
        }
    }
    layout
        .iter()
        .map(|node| IterationPhiSelectorWrite {
            node: node.node(),
            target: node.selector(),
            condition: conditions_by_selector[&node.selector()],
        })
        .collect()
}

/// 工作表收集可达图节点，每个节点只记录一次（有限布局，不按捕获路径展开）。
/// 迭代实现：菱形阶梯不指数、深链不爆栈。
fn finite_layout_order(edges: &[Vec<usize>], roots: &[usize]) -> Vec<usize> {
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

/// 释放端只能沿静态唯一、直接含 owned move 且无 shared 输入的紧邻子来源展开后代实例。
/// 与此同形的后代才能让 phi 计划发布；其余形状继续 atomic deferred。
fn descendant_expandable(nested: &IterationClosurePhiOrigin) -> bool {
    nested.sources().iter().any(|source| {
        source.mode() == ClosureCaptureMode::Owned && source.effect() == ClosureCaptureEffect::Move
    }) && !nested
        .sources()
        .iter()
        .any(|source| source.mode() == ClosureCaptureMode::Shared)
}

fn mutually_exclusive(conditions: &mut CleanupConditions, origins: &[ClosureOrigin]) -> bool {
    // 同一 capture source 的多个候选是同一槽位的互斥取值：单个 source 在任一时刻只持有其中一个。
    // phi 运输后这些候选的 condition 会变成独立 presence 位的组合，静态条件表无法再证明互补。
    let structural = origins.first().is_some_and(|first| {
        first.captured_from.is_some()
            && origins
                .iter()
                .all(|origin| origin.captured_from == first.captured_from)
    });
    // 无论结构判定如何都执行布尔检查：and() 会注册条件节点，保留该副作用以维持
    // 条件表编号与 combine() 规范化的确定性。
    let mut boolean = true;
    for (index, left) in origins.iter().enumerate() {
        for right in &origins[index + 1..] {
            if conditions.and(left.condition, right.condition) != CleanupConditionId::NEVER {
                boolean = false;
            }
        }
    }
    structural || boolean
}

fn omit_coexisting_capture_writes(origins: &mut [IterationPhiIncomingOrigin]) {
    for origin in origins {
        for environment in &mut origin.environments {
            for source in &mut environment.sources {
                // Keep the formed-instance read edge and nested presence, but never merge
                // two live children into one static phi capture slot/source owner.
                source.target = None;
                source.capture_slot = None;
                omit_coexisting_capture_writes(&mut source.captured);
            }
        }
    }
}

type SeenCaptureInstances =
    BTreeMap<usize, Vec<(CleanupOwnerValueId, Vec<usize>, CleanupConditionId)>>;

/// 同一布局节点的两个可同时成立的环境必须保留各自的实例路径。
fn coexisting_capture_node(
    conditions: &mut CleanupConditions,
    graph: &IterationCaptureGraph,
    origins: &[IterationPhiIncomingOrigin],
    seen: &mut SeenCaptureInstances,
) -> Option<usize> {
    for origin in origins {
        for environment in origin.environments() {
            let path = environment.capture_path();
            if !graph.nodes()[origin.node()].sources().is_empty() {
                let prior = seen.entry(origin.node()).or_default();
                if prior.iter().any(|(other_root, other_path, condition)| {
                    (*other_root != environment.instance_root() || other_path.as_slice() != path)
                        && conditions.and(*condition, environment.condition())
                            != CleanupConditionId::NEVER
                }) {
                    return Some(origin.node());
                }
                prior.push((
                    environment.instance_root(),
                    path.to_vec(),
                    environment.condition(),
                ));
            }
            for source in environment.sources() {
                if let Some(node) =
                    coexisting_capture_node(conditions, graph, source.captured(), seen)
                {
                    return Some(node);
                }
            }
        }
    }
    None
}

/// 只枚举有限节点的借用摘要；不沿递归 owned 边展开来源树或生成静态 loan-end。
fn seed_recursive_source_holds(
    context: &PhiSeedContext<'_, '_, '_>,
    conditions: &mut CleanupConditions,
    phi: &IterationClosurePhiBinding,
    available: CleanupConditionId,
) -> Vec<ClosureOrigin> {
    let mut held_sources: Vec<OwnerVersion> = Vec::new();
    for origin in phi.origins() {
        for capture in context
            .checker
            .captures_of(origin.closure())
            .filter(|capture| capture.mode() == ClosureCaptureMode::Shared)
        {
            let ClosureCaptureSource::Symbol(symbol) = capture.source() else {
                continue;
            };
            let Some(source) = context
                .selected
                .iter()
                .find(|source| source.symbol() == symbol)
            else {
                continue;
            };
            let present = conditions.and(available, origin.condition());
            let selected = conditions.and(present, source.availability_condition());
            if let Some(held) = held_sources
                .iter_mut()
                .find(|held| held.owner == source.owner())
            {
                held.condition = conditions.or(held.condition, selected);
            } else {
                held_sources.push(OwnerVersion {
                    owner: source.owner(),
                    condition: selected,
                    origin: capture.reference_span(),
                });
            }
        }
    }
    if held_sources.is_empty() {
        return Vec::new();
    }
    phi.root_origins()
        .map(|root| ClosureOrigin {
            inputs: Vec::new(),
            held_sources: held_sources.clone(),
            captured: Vec::new(),
            captured_from: None,
            owner: phi.owner(),
            layout_owner: phi.owner(),
            closure: root.closure(),
            condition: conditions.and(available, root.condition()),
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn seed_phi_origin(
    context: &PhiSeedContext<'_, '_, '_>,
    conditions: &mut CleanupConditions,
    phi_origin: &IterationClosurePhiOrigin,
    owner: CleanupOwnerValueId,
    layout_owner: CleanupOwnerValueId,
    parent_condition: CleanupConditionId,
    origin_layouts: &[IterationClosurePhiOrigin],
    retained_sources: &mut Vec<RetainedSource>,
) -> ClosureOrigin {
    let condition = conditions.and(parent_condition, phi_origin.condition());
    let inputs = context
        .checker
        .captures_of(phi_origin.closure())
        .flat_map(|capture| {
            let slot = phi_origin
                .sources()
                .iter()
                .find(|slot| slot.source() == capture.source());
            let enclosing = context.current_environment.and_then(|(owner, enclosing)| {
                context
                    .checker
                    .captures_of(enclosing)
                    .any(|outer| outer.source() == capture.source())
                    .then(|| {
                        let slot = conditions
                            .capture_slot(owner, capture.source())
                            .expect("checked enclosing capture has a registered slot");
                        CleanupCaptureValue::Environment {
                            owner,
                            source: capture.source(),
                            slot,
                        }
                    })
            });
            let current = match capture.source() {
                ClosureCaptureSource::Symbol(symbol)
                    if capture.mode() == ClosureCaptureMode::Shared =>
                {
                    context
                        .selected
                        .iter()
                        .find(|candidate| candidate.symbol() == symbol)
                        .copied()
                }
                _ => None,
            };
            // Header 仍为缺席 binding 预留 phi；仅可用路径能用它结束具名源。
            // 其余路径必须保留环境保存的 source，不把定义存在等同于实例存在。
            let mut inputs = Vec::new();
            let retained_condition = if enclosing.is_none()
                && let Some(current) = current
            {
                let available = current.availability_condition();
                let named_condition = conditions.and(condition, available);
                if named_condition != CleanupConditionId::NEVER {
                    inputs.push(CleanupCaptureInput {
                        source: capture.source(),
                        value: CleanupCaptureValue::Owner(current.owner()),
                        mode: capture.mode(),
                        effect: capture.effect(),
                        condition: named_condition,
                        origin: capture.reference_span(),
                    });
                }
                let unavailable = conditions.not(available);
                conditions.and(condition, unavailable)
            } else {
                condition
            };
            if retained_condition == CleanupConditionId::NEVER {
                return inputs;
            }
            let value = enclosing
                .or_else(|| slot.map(|slot| CleanupCaptureValue::Owner(slot.owner())))
                .unwrap_or(CleanupCaptureValue::Place(capture.source()));
            if enclosing.is_none()
                && capture.mode() == ClosureCaptureMode::Shared
                && let (ClosureCaptureSource::Symbol(symbol), Some(slot)) = (capture.source(), slot)
            {
                retained_sources.push(RetainedSource {
                    statement: context.statement,
                    owner: slot.owner(),
                    symbol,
                    condition: retained_condition,
                    origin: capture.reference_span(),
                });
            }
            inputs.push(CleanupCaptureInput {
                source: capture.source(),
                value,
                mode: capture.mode(),
                effect: capture.effect(),
                condition: retained_condition,
                origin: capture.reference_span(),
            });
            inputs
        })
        .collect::<Vec<_>>();
    let captured = phi_origin
        .sources()
        .iter()
        .flat_map(|slot| {
            slot.captured()
                .iter()
                .map(move |index| (slot, &origin_layouts[*index]))
        })
        .map(|(slot, origin)| {
            let mut captured = seed_phi_origin(
                context,
                conditions,
                origin,
                slot.owner(),
                layout_owner,
                condition,
                origin_layouts,
                retained_sources,
            );
            captured.captured_from = Some(slot.source());
            captured
        })
        .collect();
    ClosureOrigin {
        held_sources: Vec::new(),
        owner,
        layout_owner,
        closure: phi_origin.closure(),
        condition,
        captured,
        captured_from: None,
        inputs,
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn iteration(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        // Source 求值中的 return/abort 发生在 provider 建立之前。
        if !self.expression(source, ExpressionUse::Place, state)? {
            return Ok(false);
        }
        self.preallocate_closure_phis(statement)?;
        self.record_phi_incoming(
            statement,
            IterationPhiBoundary::Header,
            IterationPhiIncomingKind::Entry,
            DropPoint::AfterExpression(source),
            state,
        );
        let temporary_owner = self
            .checker
            .temporary_element_owner(source)?
            .unwrap_or(source);
        let temporary = if self.is_move_only_temporary(temporary_owner) {
            let origin = self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(temporary_owner)?
                .span();
            let owner = self
                .temporary_owner_version(temporary_owner, origin, state)
                .owner;
            Some((temporary_owner, origin, owner))
        } else {
            None
        };
        state.iterations.push(IterationFrame {
            statement,
            element_active: false,
            source_root: self.checker.place(source)?.map(|place| place.root()),
            temporary,
            scope_depth: self.scope_depth,
            loop_depth: self.loop_boundaries.len() + 1,
        });
        let mut body_state = state.clone();
        self.seed_phi_state(statement, IterationPhiBoundary::Header, &mut body_state);
        if let Some(frame) = body_state.iterations.last_mut() {
            frame.element_active = true;
        }
        self.loop_boundaries.push(self.scope_depth);
        if self.statement(body, &mut body_state)? {
            self.end_iteration_element(
                DropPoint::AfterStatement(body),
                IterationExitKind::Fallthrough,
                &mut body_state,
            );
            self.record_phi_incoming(
                statement,
                IterationPhiBoundary::Header,
                IterationPhiIncomingKind::Fallthrough,
                DropPoint::AfterStatement(body),
                &body_state,
            );
        }
        self.loop_boundaries.pop();
        // Exhaustion 是独立的可能路径，包括零次迭代。
        let mut exhaustion_state = state.clone();
        self.seed_phi_state(
            statement,
            IterationPhiBoundary::Header,
            &mut exhaustion_state,
        );
        self.finish_iteration(
            DropPoint::LoopExit(statement),
            IterationExitKind::Exhaustion,
            &mut exhaustion_state,
        );
        self.drop_loop_exit(statement, &mut exhaustion_state);
        self.record_exhaustion_incoming(statement, &exhaustion_state);
        self.seed_phi_state(statement, IterationPhiBoundary::Exit, &mut exhaustion_state);
        *state = exhaustion_state;
        Ok(true)
    }

    /// header/exit 必须在 body 的任何选择、snapshot 或跳转之前取得独立身份。
    fn preallocate_closure_phis(
        &mut self,
        statement: StatementId,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(summary) = self.loop_origins.get(&statement.index()).cloned() else {
            return Ok(());
        };
        let mut graph = PhiCaptureGraph::default();
        for closure in [summary.header(), summary.exit()]
            .into_iter()
            .flatten()
            .flat_map(|binding| binding.origins())
        {
            graph.insert(*closure);
        }
        graph.expand(self);
        let conditional_nested_nodes = graph.conditional_nested_nodes(&self.captured_origins);
        if let Some(node) = conditional_nested_nodes
            .iter()
            .position(|&required| required)
        {
            self.conditional_nested_phi
                .get_or_insert(graph.nodes[node].closure);
        }
        self.loop_capture_graphs
            .insert(statement.index(), graph.published());
        let recursive = graph.recursive_origin();
        let recursive_nodes = graph.nodes_reaching_cycle();
        let recursive_release_nodes = graph.owned_nodes_reaching_cycle();
        let origin = self
            .checker
            .parsed
            .ast()
            .statements()
            .get(statement)?
            .span();
        let mut phis = Vec::new();
        for (boundary, bindings) in [
            (IterationPhiBoundary::Header, summary.header()),
            (IterationPhiBoundary::Exit, summary.exit()),
        ] {
            let allocation = PhiAllocation {
                statement,
                boundary,
                origin,
            };
            let mut slot = 0;
            for binding in bindings {
                let owner = self
                    .conditions
                    .create_owner(CleanupOwnerValue::IterationPhi {
                        statement,
                        boundary,
                        symbol: binding.symbol(),
                        origin,
                    });
                let capture_layout =
                    graph.register_capture_layout(binding.origins(), owner, &mut self.conditions);
                let root_nodes: Vec<usize> = binding
                    .origins()
                    .iter()
                    .map(|closure| graph.by_closure[&closure.index()])
                    .collect();
                let instance_presence_required = graph
                    .reachable(binding.origins())
                    .into_iter()
                    .any(|node| conditional_nested_nodes[node]);
                if root_nodes.iter().any(|&node| recursive_nodes[node]) {
                    self.recursive_phi_bindings.insert(owner);
                }
                if root_nodes.iter().any(|&node| recursive_release_nodes[node])
                    || (root_nodes.iter().all(|&node| !recursive_nodes[node])
                        && graph.coexisting_owned_roots(&root_nodes, self))
                {
                    self.recursive_release_phi_bindings.insert(owner);
                }
                let availability_condition = self
                    .conditions
                    .iteration_presence(statement, boundary, slot, origin);
                let Some(CleanupCondition::Choice {
                    selector: availability_selector,
                    ..
                }) = self.conditions.get(availability_condition)
                else {
                    unreachable!("iteration availability has two distinct branches")
                };
                let availability_selector = *availability_selector;
                slot += 1;
                let origins =
                    self.allocate_phi_layout(allocation, &graph, &root_nodes, owner, &mut slot);
                phis.push(IterationClosurePhiBinding {
                    boundary,
                    symbol: binding.symbol(),
                    owner,
                    root_nodes,
                    capture_layout,
                    availability_selector,
                    availability_condition,
                    instance_presence_required,
                    origins,
                });
            }
        }
        if let Some(recursive) = recursive {
            self.recursive_capture_phi.get_or_insert(recursive);
            // 有限布局已分配，但树形来源不可沿环展开；body 仍从 header owner 读取旧实例。
            // 这些内部 phi 随整文件 deferred 丢弃，不能作为可执行计划发布。
            self.loop_phis.insert(statement.index(), phis);
            return Ok(());
        }
        self.loop_phis.insert(statement.index(), phis);
        Ok(())
    }

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

    pub(super) fn phi_root_conditions(
        &self,
        owner: CleanupOwnerValueId,
    ) -> Vec<(ExpressionId, CleanupOwnerValueId, CleanupConditionId)> {
        if let Some(roots) = self.snapshot_phi_roots.get(&owner) {
            return roots.clone();
        }
        self.loop_phis
            .values()
            .flat_map(|bindings| bindings.iter())
            .find(|binding| binding.owner() == owner)
            .map(|binding| {
                binding
                    .root_origins()
                    .map(|root| (root.closure(), owner, root.condition()))
                    .collect()
            })
            .unwrap_or_default()
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

    /// 工作表收集可达节点，每个节点只分配一次存在位与来源槽。
    /// 后代只保存图节点索引，不再按路径递归展开（避免菱形指数与深链栈耗尽）。
    fn allocate_phi_layout(
        &mut self,
        allocation: PhiAllocation,
        graph: &PhiCaptureGraph,
        roots: &[usize],
        owner: crate::ownership_checking::CleanupOwnerValueId,
        slot: &mut usize,
    ) -> Vec<IterationClosurePhiOrigin> {
        let edges: Vec<Vec<usize>> = graph
            .nodes
            .iter()
            .map(|node| {
                node.sources
                    .iter()
                    .flat_map(|(_, _, captured)| captured.iter().copied())
                    .collect()
            })
            .collect();
        let order = finite_layout_order(&edges, roots);
        let index_of: BTreeMap<usize, usize> = order
            .iter()
            .enumerate()
            .map(|(index, node)| (*node, index))
            .collect();
        let mut layout = Vec::with_capacity(order.len());
        for &node in &order {
            let closure = graph.nodes[node].closure;
            let condition = self.conditions.iteration_presence(
                allocation.statement,
                allocation.boundary,
                *slot,
                allocation.origin,
            );
            let Some(CleanupCondition::Choice { selector, .. }) = self.conditions.get(condition)
            else {
                unreachable!("iteration presence always has two distinct branches")
            };
            let selector = *selector;
            *slot += 1;
            let mut sources = Vec::new();
            for (capture, _, candidates) in &graph.nodes[node].sources {
                let source_owner =
                    self.conditions
                        .create_owner(CleanupOwnerValue::IterationPhiSourceOwner {
                            environment: owner,
                            closure,
                            source: capture.source(),
                            origin: capture.reference_span(),
                        });
                let captured = candidates
                    .iter()
                    .map(|&candidate| index_of[&candidate])
                    .collect();
                sources.push(IterationClosurePhiSource {
                    source: capture.source(),
                    mode: capture.mode(),
                    effect: capture.effect(),
                    owner: source_owner,
                    captured,
                });
            }
            layout.push(IterationClosurePhiOrigin {
                node,
                closure,
                selector,
                condition,
                sources,
            });
        }
        layout
    }

    /// body/后继分别从 header/exit 动态槽重建，不复用零轮入口的 owner 定义。
    fn seed_phi_state(
        &mut self,
        statement: StatementId,
        boundary: IterationPhiBoundary,
        state: &mut ValueState,
    ) {
        let Some(phis) = self.loop_phis.get(&statement.index()) else {
            return;
        };
        let selected = phis
            .iter()
            .filter(|phi| phi.boundary() == boundary)
            .filter(|phi| {
                boundary == IterationPhiBoundary::Header
                    || self.exit_binding_required(statement, phi.symbol(), state)
            })
            .collect::<Vec<_>>();
        if boundary == IterationPhiBoundary::Exit {
            for header in phis
                .iter()
                .filter(|phi| phi.boundary() == IterationPhiBoundary::Header)
            {
                state.take(header.symbol());
            }
        }
        state
            .retained_sources
            .retain(|source| source.statement != statement);
        let context = PhiSeedContext {
            checker: self.checker,
            current_environment: self.current_environment,
            selected: &selected,
            statement,
        };
        for phi in &selected {
            let symbol = phi.symbol();
            let origin = self.checker.names.symbols()[symbol.index()].span();
            let available_when = self
                .conditions
                .and(state.path, phi.availability_condition());
            let scope_depth = self
                .binding_depths
                .get(&symbol)
                .copied()
                .unwrap_or(self.scope_depth);
            state.insert(OwnedValue {
                versions: vec![OwnerVersion {
                    owner: phi.owner(),
                    condition: available_when,
                    origin,
                }],
                condition: available_when,
                symbol,
                origin,
                declaration: origin,
                scope_depth,
            });
            let origins = if self.recursive_phi_bindings.contains(&phi.owner()) {
                seed_recursive_source_holds(&context, &mut self.conditions, phi, available_when)
            } else {
                phi.root_origins()
                    .map(|phi_origin| {
                        seed_phi_origin(
                            &context,
                            &mut self.conditions,
                            phi_origin,
                            phi.owner(),
                            phi.owner(),
                            state.path,
                            phi.origins(),
                            &mut state.retained_sources,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            if origins.is_empty() {
                state.closures.remove(&symbol);
            } else {
                state.closures.insert(symbol, origins);
            }
        }
    }

    /// 循环后仍被外层上下文持有的 binding 不能仅因缺少源码后继读取而写 false。
    fn exit_binding_required(
        &self,
        statement: StatementId,
        symbol: SymbolId,
        state: &ValueState,
    ) -> bool {
        self.liveness.statement_after[statement.index()].contains(&symbol)
            || (state.position(symbol).is_some()
                && (self.owner_protected_by_context(symbol, state)
                    || state.replacements.contains(&symbol)
                    || state
                        .values
                        .iter()
                        .filter(|value| value.symbol == symbol)
                        .flat_map(|value| &value.versions)
                        .any(|version| {
                            state
                                .closures
                                .values()
                                .flatten()
                                .any(|origin| origin.holds_recursive_source(version.owner))
                        })))
    }

    /// 所有输入都读取保存前的 ValueState；缺席来源写入 false，不能继承上轮位值。
    fn record_phi_incoming(
        &mut self,
        statement: StatementId,
        boundary: IterationPhiBoundary,
        kind: IterationPhiIncomingKind,
        point: DropPoint,
        state: &ValueState,
    ) {
        let Some(phis) = self.loop_phis.get(&statement.index()) else {
            return;
        };
        let mut bindings = Vec::new();
        for phi in phis.iter().filter(|phi| phi.boundary() == boundary) {
            let survives_exit = boundary != IterationPhiBoundary::Exit
                || self.exit_binding_required(statement, phi.symbol(), state);
            let value = state
                .values
                .iter()
                .find(|value| survives_exit && value.symbol == phi.symbol());
            let mut available_when = CleanupConditionId::NEVER;
            let values: Vec<IterationPhiIncomingValue> = value
                .map(|value| {
                    value
                        .versions
                        .iter()
                        .filter(|version| version.condition != CleanupConditionId::NEVER)
                        .map(|version| {
                            available_when = self.conditions.or(available_when, version.condition);
                            IterationPhiIncomingValue {
                                source: version.owner,
                                condition: version.condition,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let candidates = state
                .closures
                .get(&phi.symbol())
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let mut root_sources = Vec::new();
            if let Some(graph) = self.loop_capture_graphs.get(&statement.index()) {
                for &node in phi.root_nodes() {
                    for candidate in &candidates {
                        if candidate.closure != graph.nodes()[node].closure() {
                            continue;
                        }
                        let condition = self.conditions.and(candidate.condition, available_when);
                        if condition != CleanupConditionId::NEVER {
                            root_sources.push(IterationPhiRootSource {
                                node,
                                source: candidate.owner,
                                condition,
                            });
                        }
                    }
                }
                for value in &values {
                    // 递归 phi 无树形来源；Snapshot 也须复制其根选择位后再转运。
                    for (closure, source_owner, source_when) in
                        self.phi_root_conditions(value.source())
                    {
                        if candidates.iter().any(|candidate| {
                            candidate.owner == value.source()
                                && candidate.layout_owner == source_owner
                                && candidate.closure == closure
                        }) {
                            continue;
                        }
                        let Some(&node) = phi
                            .root_nodes()
                            .iter()
                            .find(|&&node| graph.nodes()[node].closure() == closure)
                        else {
                            continue;
                        };
                        let condition = self.conditions.and(value.condition(), source_when);
                        if condition != CleanupConditionId::NEVER {
                            root_sources.push(IterationPhiRootSource {
                                node,
                                source: value.source(),
                                condition,
                            });
                        }
                    }
                }
            }
            let mut origins = if self.recursive_phi_bindings.contains(&phi.owner()) {
                Vec::new()
            } else {
                phi.root_origins()
                    .map(|origin| {
                        record_phi_origin(
                            &mut self.conditions,
                            phi.owner(),
                            origin,
                            &candidates,
                            available_when,
                            None,
                            phi.origins(),
                            &mut self.enclosing_capture_phi,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let selector_writes =
                phi_selector_writes(&mut self.conditions, phi.origins(), &origins, &root_sources);
            let coexisting = self
                .loop_capture_graphs
                .get(&statement.index())
                .and_then(|graph| {
                    coexisting_capture_node(
                        &mut self.conditions,
                        graph,
                        &origins,
                        &mut BTreeMap::new(),
                    )
                    .map(|node| graph.nodes()[node].closure())
                });
            if let Some(closure) = coexisting {
                self.coexisting_capture_phi.get_or_insert(closure);
                omit_coexisting_capture_writes(&mut origins);
            }
            bindings.push(IterationPhiIncomingBinding {
                target: phi.owner(),
                capture_slots_to_clear: if self.recursive_phi_bindings.contains(&phi.owner())
                    || phi.origins().is_empty()
                    || coexisting.is_some()
                {
                    // Instance-qualified descendants stay in the formed environment.
                    Vec::new()
                } else {
                    phi.capture_layout()
                        .iter()
                        .map(|slot| slot.slot())
                        .collect()
                },
                availability_selector: phi.availability_selector(),
                available_when,
                values,
                root_sources,
                presence_source: if phi.instance_presence_required
                    || self.recursive_phi_bindings.contains(&phi.owner())
                    || coexisting.is_some()
                {
                    IterationPhiPresenceSource::CapturedInstances
                } else {
                    IterationPhiPresenceSource::StaticConditions
                },
                selector_writes,
                origins,
            });
        }
        self.loop_phi_incomings
            .entry(statement.index())
            .or_default()
            .push(IterationPhiIncoming {
                kind,
                point,
                boundary,
                condition: state.path,
                bindings,
            });
    }

    /// 零轮及后续耗尽均读取已初始化的 header 槽，而非重新读取入口 binding。
    fn record_exhaustion_incoming(&mut self, statement: StatementId, state: &ValueState) {
        let Some(phis) = self.loop_phis.get(&statement.index()) else {
            return;
        };
        let mut bindings = Vec::new();
        for exit in phis
            .iter()
            .filter(|phi| phi.boundary() == IterationPhiBoundary::Exit)
        {
            let header = phis.iter().find(|phi| {
                self.exit_binding_required(statement, exit.symbol(), state)
                    && phi.boundary() == IterationPhiBoundary::Header
                    && phi.symbol() == exit.symbol()
            });
            let available_when = header
                .and_then(|header| {
                    state
                        .values
                        .iter()
                        .find(|value| value.symbol == exit.symbol())
                        .and_then(|value| {
                            value
                                .versions
                                .iter()
                                .find(|version| version.owner == header.owner())
                        })
                        .map(|version| {
                            // 未被出口清理改变时复用原 header 条件；入边自身已保护外层 path。
                            let initial = self
                                .conditions
                                .and(state.path, header.availability_condition());
                            if version.condition == initial {
                                header.availability_condition()
                            } else {
                                version.condition
                            }
                        })
                })
                .unwrap_or(CleanupConditionId::NEVER);
            let values = header
                .filter(|_| available_when != CleanupConditionId::NEVER)
                .map(|header| {
                    vec![IterationPhiIncomingValue {
                        source: header.owner(),
                        condition: available_when,
                    }]
                })
                .unwrap_or_default();
            let root_sources = exit
                .root_nodes()
                .iter()
                .filter_map(|&node| {
                    let header = header?;
                    let source = header.root_origins().find(|origin| origin.node() == node)?;
                    let condition = self.conditions.and(source.condition(), available_when);
                    (condition != CleanupConditionId::NEVER).then_some(IterationPhiRootSource {
                        node,
                        source: header.owner(),
                        condition,
                    })
                })
                .collect::<Vec<_>>();
            let mut origins = if self.recursive_phi_bindings.contains(&exit.owner()) {
                Vec::new()
            } else {
                exit.root_origins()
                    .map(|target| {
                        let source = header.and_then(|header| {
                            header
                                .root_origins()
                                .find(|origin| origin.closure() == target.closure())
                        });
                        Self::forward_phi_origin(
                            &mut ForwardPhiContext {
                                conditions: &mut self.conditions,
                                checker: self.checker,
                            },
                            exit.owner(),
                            target,
                            source,
                            header.map(|header| header.owner()),
                            header.map(|header| header.owner()),
                            None,
                            exit.origins(),
                            header.map(|header| header.origins()).unwrap_or(&[]),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let selector_writes = phi_selector_writes(
                &mut self.conditions,
                exit.origins(),
                &origins,
                &root_sources,
            );
            let coexisting = self
                .loop_capture_graphs
                .get(&statement.index())
                .and_then(|graph| {
                    coexisting_capture_node(
                        &mut self.conditions,
                        graph,
                        &origins,
                        &mut BTreeMap::new(),
                    )
                    .map(|node| graph.nodes()[node].closure())
                });
            if let Some(closure) = coexisting {
                self.coexisting_capture_phi.get_or_insert(closure);
                omit_coexisting_capture_writes(&mut origins);
            }
            bindings.push(IterationPhiIncomingBinding {
                target: exit.owner(),
                capture_slots_to_clear: if self.recursive_phi_bindings.contains(&exit.owner())
                    || exit.origins().is_empty()
                    || coexisting.is_some()
                {
                    Vec::new()
                } else {
                    exit.capture_layout()
                        .iter()
                        .map(|slot| slot.slot())
                        .collect()
                },
                availability_selector: exit.availability_selector(),
                available_when,
                values,
                root_sources,
                presence_source: if exit.instance_presence_required
                    || self.recursive_phi_bindings.contains(&exit.owner())
                    || coexisting.is_some()
                {
                    IterationPhiPresenceSource::CapturedInstances
                } else {
                    IterationPhiPresenceSource::StaticConditions
                },
                selector_writes,
                origins,
            });
        }
        self.loop_phi_incomings
            .entry(statement.index())
            .or_default()
            .push(IterationPhiIncoming {
                kind: IterationPhiIncomingKind::Exhaustion,
                point: DropPoint::LoopExit(statement),
                boundary: IterationPhiBoundary::Exit,
                condition: state.path,
                bindings,
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn forward_phi_origin(
        context: &mut ForwardPhiContext<'_, '_>,
        root: CleanupOwnerValueId,
        target: &IterationClosurePhiOrigin,
        source: Option<&IterationClosurePhiOrigin>,
        source_owner: Option<crate::ownership_checking::CleanupOwnerValueId>,
        source_layout_owner: Option<CleanupOwnerValueId>,
        instance_path: Option<(CleanupOwnerValueId, Vec<usize>)>,
        target_layouts: &[IterationClosurePhiOrigin],
        source_layouts: &[IterationClosurePhiOrigin],
    ) -> IterationPhiIncomingOrigin {
        let condition = source
            .map(|source| source.condition())
            .unwrap_or(CleanupConditionId::NEVER);
        let environments = source
            .zip(source_owner)
            .map(|(source, source_owner)| {
                let (instance_root, capture_path) = instance_path
                    .clone()
                    .unwrap_or_else(|| (source_owner, Vec::new()));
                let sources = context
                    .checker
                    .captures_of(target.closure())
                    .enumerate()
                    .map(|(position, capture)| {
                        let prior = source
                            .sources()
                            .iter()
                            .find(|slot| slot.source() == capture.source());
                        let destination = target
                            .sources()
                            .iter()
                            .find(|slot| slot.source() == capture.source());
                        let value = prior
                            .map(|slot| CleanupCaptureValue::Owner(slot.owner()))
                            .unwrap_or(CleanupCaptureValue::Place(capture.source()));
                        let captured = destination
                            .into_iter()
                            .flat_map(|slot| slot.captured().iter().copied())
                            .map(|target_index| {
                                let nested = &target_layouts[target_index];
                                let previous = prior.and_then(|prior| {
                                    prior
                                        .captured()
                                        .iter()
                                        .copied()
                                        .find(|&source_index| {
                                            source_layouts[source_index].closure()
                                                == nested.closure()
                                        })
                                        .map(|source_index| &source_layouts[source_index])
                                });
                                Self::forward_phi_origin(
                                    context,
                                    root,
                                    nested,
                                    previous,
                                    prior.map(|prior| prior.owner()),
                                    source_layout_owner,
                                    Some((instance_root, {
                                        let mut path = capture_path.clone();
                                        path.push(position);
                                        path
                                    })),
                                    target_layouts,
                                    source_layouts,
                                )
                            })
                            .collect();
                        IterationPhiIncomingSource {
                            target: destination.and_then(|slot| prior.map(|_| slot.owner())),
                            capture_slot: destination.map(|_| {
                                context
                                    .conditions
                                    .phi_capture_slot(root, target.closure(), capture.source())
                                    .expect("preallocated phi capture has a registered layout slot")
                            }),
                            source_capture_slot: prior.map(|_| {
                                context
                                    .conditions
                                    .phi_capture_slot(
                                        source_layout_owner
                                            .expect("forwarded phi has source layout"),
                                        source.closure(),
                                        capture.source(),
                                    )
                                    .expect("preallocated source phi capture has a layout slot")
                            }),
                            read_address: prior.map(|_| {
                                context
                                    .conditions
                                    .register_instance_address(instance_root, &capture_path)
                            }),
                            source_environment: source_owner,
                            nested_instance: !capture_path.is_empty(),
                            input: CleanupCaptureInput {
                                source: capture.source(),
                                value,
                                mode: capture.mode(),
                                effect: capture.effect(),
                                condition,
                                origin: capture.reference_span(),
                            },
                            captured,
                        }
                    })
                    .collect();
                vec![IterationPhiIncomingEnvironment {
                    owner: source_owner,
                    instance_root,
                    capture_path,
                    condition,
                    sources,
                }]
            })
            .unwrap_or_default();
        IterationPhiIncomingOrigin {
            node: target.node(),
            target: target.selector(),
            condition,
            environments,
        }
    }

    pub(super) fn end_iteration_element(
        &mut self,
        point: DropPoint,
        kind: IterationExitKind,
        state: &mut ValueState,
    ) {
        let Some(frame) = state.iterations.last_mut() else {
            return;
        };
        self.iteration_exits.push((frame.statement, kind, point));
        if !frame.element_active {
            return;
        }
        if let Some(plan) = self.checker.iterations.get(&frame.statement.index()) {
            for binding in plan.bindings().iter().rev() {
                self.cleanup.push((
                    point,
                    Action::EndBinding {
                        statement: frame.statement,
                        symbol: binding.symbol(),
                    },
                ));
            }
        }
        self.cleanup
            .push((point, Action::EndElement(frame.statement)));
        frame.element_active = false;
        if let IterationExitKind::Continue(expression) = kind {
            self.record_phi_incoming(
                frame.statement,
                IterationPhiBoundary::Header,
                IterationPhiIncomingKind::Continue(expression),
                point,
                state,
            );
        }
    }

    pub(super) fn finish_iteration(
        &mut self,
        point: DropPoint,
        kind: IterationExitKind,
        state: &mut ValueState,
    ) {
        self.end_iteration_element(point, kind, state);
        if let Some(frame) = state.iterations.last() {
            self.cleanup
                .push((point, Action::FinishProvider(frame.statement)));
            self.cleanup
                .push((point, Action::EndSource(frame.statement)));
        }
        if let Some(frame) = state.iterations.pop() {
            if let Some((source, origin, owner)) = frame.temporary {
                self.push_fact(
                    DropFact::new(point, DropTarget::Temporary(source), origin).with_owner(owner),
                );
            }
            if matches!(kind, IterationExitKind::Break(_)) {
                self.drop_loop_exit_at(frame.statement, point, state);
                if let IterationExitKind::Break(expression) = kind {
                    self.record_phi_incoming(
                        frame.statement,
                        IterationPhiBoundary::Exit,
                        IterationPhiIncomingKind::Break(expression),
                        point,
                        state,
                    );
                }
            }
        }
    }

    /// 只有实际 drop traversal 覆盖所有已检查 provider 时才发布完整集合。
    pub(super) fn iteration_plans(&self) -> Vec<IterationOwnershipPlan> {
        if !self.checker.deferred.is_empty() || !self.checker.typed.diagnostics().is_empty() {
            return Vec::new();
        }
        let mut plans = Vec::new();
        for template in self.checker.iterations.values() {
            let statement = template.descriptor().statement();
            let mut plan = template.clone();
            let Some(origins) = self.loop_origins.get(&statement.index()) else {
                return Vec::new();
            };
            plan.closure_flow = origins.clone();
            let Some(capture_graph) = self.loop_capture_graphs.get(&statement.index()) else {
                return Vec::new();
            };
            plan.capture_graph = capture_graph.clone();
            let Some(phis) = self.loop_phis.get(&statement.index()) else {
                return Vec::new();
            };
            plan.closure_phis = phis.clone();
            plan.closure_phi_incomings = self
                .loop_phi_incomings
                .get(&statement.index())
                .cloned()
                .unwrap_or_default();
            for &(owner, kind, point) in &self.iteration_exits {
                if owner == statement {
                    let exit = IterationExitPlan {
                        kind,
                        point,
                        actions: self
                            .cleanup
                            .iter()
                            .filter(|(at, _)| *at == point)
                            .map(|(_, action)| *action)
                            .collect(),
                    };
                    if !plan.exits.contains(&exit) {
                        plan.exits.push(exit);
                    }
                }
            }
            if !plan
                .exits
                .iter()
                .any(|exit| exit.kind == IterationExitKind::Exhaustion)
            {
                return Vec::new();
            }
            plans.push(plan);
        }
        plans
    }
}

#[cfg(test)]
mod instance_replay;

#[cfg(test)]
mod tests;
