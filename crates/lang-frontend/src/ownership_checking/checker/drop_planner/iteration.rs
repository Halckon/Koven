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
            if matches!(input.value, CleanupCaptureValue::Environment { .. })
                && (slot.is_some_and(|slot| {
                    slot.captured()
                        .iter()
                        .any(|nested| !origin_layouts[*nested].sources().is_empty())
                }) || candidate
                    .captured
                    .iter()
                    .filter(|origin| origin.captured_from == Some(input.source))
                    .take(2)
                    .count()
                    > 1)
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
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{
        DropPoint, DropTarget, PhiCaptureGraph, coexisting_capture_node, cyclic_node,
        finite_layout_order, phi_selector_writes,
    };
    use crate::{
        ast::ExpressionId,
        name_resolution::SymbolId,
        ownership_checking::{
            CleanupCaptureInput, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
            CleanupConditions, CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorSource,
            ClosureCaptureDescriptor, ClosureCaptureEffect, ClosureCaptureMode,
            ClosureCaptureSource, ClosureReleaseLayout, IterationCaptureGraph,
            IterationCaptureNode, IterationCaptureSource, IterationCleanupAction,
            IterationClosurePhiBinding, IterationClosurePhiOrigin, IterationPhiBoundary,
            IterationPhiIncomingBinding, IterationPhiIncomingEnvironment, IterationPhiIncomingKind,
            IterationPhiIncomingOrigin, IterationPhiIncomingSource, IterationPhiPresenceSource,
        },
        parser::Expression,
        source::SourceMap,
        type_checking::TypeId,
    };

    /// 测试回放只处理夹具中的已知 owned closure 边；实例/槽来自形成动作。
    fn replay_owned_closure_release(
        graph: &IterationCaptureGraph,
        instance_nodes: &BTreeMap<usize, usize>,
        captured: &mut BTreeMap<(usize, usize), usize>,
        root: usize,
    ) -> Vec<usize> {
        let mut pending = vec![(root, false)];
        let mut visited = BTreeSet::new();
        let mut released = Vec::new();
        while let Some((instance, finished)) = pending.pop() {
            if finished {
                released.push(instance);
                continue;
            }
            assert!(
                visited.insert(instance),
                "an owned environment was reached twice"
            );
            pending.push((instance, true));
            for source in graph.nodes()[instance_nodes[&instance]].sources() {
                if source.capture().mode() != ClosureCaptureMode::Owned
                    || source.capture().effect() != ClosureCaptureEffect::Move
                {
                    continue;
                }
                assert!(!source.captured().is_empty(), "fixture needs a known child");
                let child = captured.remove(&(instance, source.position())).unwrap();
                assert!(source.captured().contains(&instance_nodes[&child]));
                pending.push((child, false));
            }
        }
        released
    }

    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                let arm = choices.get(selector).unwrap_or_else(|| {
                    panic!(
                        "missing selector {selector:?} {:?} for {condition:?}",
                        table.selector(*selector)
                    )
                });
                selected(table, branches[*arm], choices)
            }
        }
    }

    /// 静态条件入边的完整回放；实例入边必须读取当次捕获槽。
    fn replay_edge_presence(
        table: &CleanupConditions,
        edge: &crate::ownership_checking::IterationPhiIncoming,
        choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    ) {
        assert!(
            edge.bindings().iter().all(|binding| {
                binding.presence_source() == IterationPhiPresenceSource::StaticConditions
            }),
            "dynamic presence needs the selected root instance and saved capture slots"
        );
        replay_edge_candidates(table, edge, choices);
    }

    /// 仅供未完成动态实例验收的局部夹具读取静态候选，不能证明实际后代存在。
    fn replay_edge_candidates(
        table: &CleanupConditions,
        edge: &crate::ownership_checking::IterationPhiIncoming,
        choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    ) {
        let before = choices.clone();
        assert!(selected(table, edge.condition(), &before));
        let mut writes = Vec::new();
        for binding in edge.bindings() {
            writes.push((
                binding.availability_selector(),
                usize::from(selected(table, binding.available_when(), &before)),
            ));
            for write in binding.selector_writes() {
                writes.push((
                    write.target(),
                    usize::from(selected(table, write.condition(), &before)),
                ));
            }
        }
        for (target, value) in writes {
            choices.insert(target, value);
        }
    }

    /// 从旧状态选根句柄，再按实例保存的捕获槽写入实际可达节点。
    fn replay_captured_edge_presence(
        table: &CleanupConditions,
        graph: &IterationCaptureGraph,
        layout: &IterationClosurePhiBinding,
        edge: &crate::ownership_checking::IterationPhiIncoming,
        target: CleanupOwnerValueId,
        values: &BTreeMap<CleanupOwnerValueId, usize>,
        instance_nodes: &BTreeMap<usize, usize>,
        captured: &BTreeMap<(usize, usize), usize>,
        choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    ) -> (CleanupOwnerValueId, usize) {
        let before = choices.clone();
        assert!(selected(table, edge.condition(), &before));
        let binding = edge
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap();
        assert_eq!(layout.owner(), target);
        assert_eq!(
            binding.presence_source(),
            IterationPhiPresenceSource::CapturedInstances
        );
        assert!(selected(table, binding.available_when(), &before));
        let selected_values = binding
            .values()
            .iter()
            .filter(|value| selected(table, value.condition(), &before))
            .collect::<Vec<_>>();
        let [value] = selected_values.as_slice() else {
            panic!("one owner value must reach this edge")
        };
        let instance = values[&value.source()];
        let roots = binding
            .root_sources()
            .iter()
            .filter(|root| {
                root.source() == value.source()
                    && root.node() == instance_nodes[&instance]
                    && selected(table, root.condition(), &before)
            })
            .collect::<Vec<_>>();
        let [root] = roots.as_slice() else {
            panic!("the selected instance must match exactly one root candidate")
        };
        assert_eq!(instance_nodes[&instance], root.node());
        let mut pending = vec![instance];
        let mut visited = BTreeSet::new();
        let mut reached = BTreeSet::new();
        while let Some(parent) = pending.pop() {
            assert!(visited.insert(parent), "an instance was reached twice");
            let node = instance_nodes[&parent];
            reached.insert(node);
            for source in graph.nodes()[node].sources() {
                if let Some(&child) = captured.get(&(parent, source.position())) {
                    assert!(source.captured().contains(&instance_nodes[&child]));
                    pending.push(child);
                }
            }
        }
        let expected_writes = layout
            .origins()
            .iter()
            .map(|origin| (origin.node(), origin.selector()))
            .collect::<BTreeMap<_, _>>();
        let actual_writes = binding
            .selector_writes()
            .iter()
            .map(|write| (write.node(), write.target()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(layout.origins().len(), expected_writes.len());
        assert_eq!(binding.selector_writes().len(), actual_writes.len());
        assert_eq!(actual_writes, expected_writes);
        assert!(
            reached
                .iter()
                .all(|node| expected_writes.contains_key(node))
        );
        let writes = binding
            .selector_writes()
            .iter()
            .map(|write| (write.target(), usize::from(reached.contains(&write.node()))))
            .collect::<Vec<_>>();
        choices.insert(binding.availability_selector(), 1);
        for (selector, present) in writes {
            choices.insert(selector, present);
        }
        (value.source(), instance)
    }

    /// 同一入边的全部 phi 从同一旧状态读取，缺席 binding 也必须清空完整布局。
    fn replay_captured_edge(
        table: &CleanupConditions,
        graph: &IterationCaptureGraph,
        layouts: &[IterationClosurePhiBinding],
        edge: &crate::ownership_checking::IterationPhiIncoming,
        values: &BTreeMap<CleanupOwnerValueId, usize>,
        instance_nodes: &BTreeMap<usize, usize>,
        captured: &BTreeMap<(usize, usize), usize>,
        choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    ) -> BTreeMap<CleanupOwnerValueId, (CleanupOwnerValueId, usize)> {
        let before = choices.clone();
        assert!(selected(table, edge.condition(), &before));
        let mut writes = BTreeMap::new();
        let mut transported = BTreeMap::new();
        for binding in edge.bindings() {
            let layout = layouts
                .iter()
                .find(|layout| layout.owner() == binding.target())
                .unwrap();
            assert_eq!(layout.boundary(), edge.boundary());
            let expected = layout
                .origins()
                .iter()
                .map(|origin| (origin.node(), origin.selector()))
                .collect::<BTreeMap<_, _>>();
            let actual = binding
                .selector_writes()
                .iter()
                .map(|write| (write.node(), write.target()))
                .collect::<BTreeMap<_, _>>();
            assert_eq!(expected.len(), layout.origins().len());
            assert_eq!(actual.len(), binding.selector_writes().len());
            assert_eq!(actual, expected);
            let selected_values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &before))
                .count();
            let available = selected(table, binding.available_when(), &before);
            assert_eq!(selected_values, usize::from(available));
            let mut after = before.clone();
            match binding.presence_source() {
                IterationPhiPresenceSource::StaticConditions => {
                    after.insert(binding.availability_selector(), usize::from(available));
                    for write in binding.selector_writes() {
                        let present = selected(table, write.condition(), &before);
                        assert!(available || !present);
                        after.insert(write.target(), usize::from(present));
                    }
                    if available {
                        let value = binding
                            .values()
                            .iter()
                            .find(|value| selected(table, value.condition(), &before))
                            .unwrap();
                        if let Some(&instance) = values.get(&value.source()) {
                            assert!(
                                transported
                                    .insert(binding.target(), (value.source(), instance))
                                    .is_none()
                            );
                        } else {
                            // 本夹具只给 closure 分配实例；无根布局的普通 source 仍写选择位。
                            assert!(layout.origins().is_empty());
                            assert!(binding.root_sources().is_empty());
                        }
                    }
                }
                IterationPhiPresenceSource::CapturedInstances if available => {
                    let source = replay_captured_edge_presence(
                        table,
                        graph,
                        layout,
                        edge,
                        binding.target(),
                        values,
                        instance_nodes,
                        captured,
                        &mut after,
                    );
                    assert!(transported.insert(binding.target(), source).is_none());
                }
                IterationPhiPresenceSource::CapturedInstances => {
                    after.insert(binding.availability_selector(), 0);
                    for write in binding.selector_writes() {
                        after.insert(write.target(), 0);
                    }
                }
            }
            assert!(
                writes
                    .insert(
                        binding.availability_selector(),
                        after[&binding.availability_selector()],
                    )
                    .is_none()
            );
            for write in binding.selector_writes() {
                assert!(
                    writes
                        .insert(write.target(), after[&write.target()])
                        .is_none()
                );
            }
        }
        choices.extend(writes);
        transported
    }

    fn replay_snapshot_choices(
        table: &CleanupConditions,
        owner: CleanupOwnerValueId,
        choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    ) {
        let snapshot = table.owner_snapshot(owner).unwrap();
        let before = choices.clone();
        let writes = snapshot
            .copies()
            .iter()
            .filter(|copy| selected(table, copy.when(), &before))
            .map(|copy| {
                assert!(
                    copy.source_value().is_none(),
                    "this flat replay needs a direct selector source"
                );
                (copy.target(), before[&copy.source()])
            })
            .collect::<Vec<_>>();
        for (target, value) in writes {
            choices.insert(target, value);
        }
    }

    #[test]
    fn coexisting_capture_paths_include_the_root_instance() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("capture-roots.ko", "fun run() { val f = move {} }")
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        let (closure, span) = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                matches!(node.payload(), Expression::Lambda { .. }).then_some((id, node.span()))
            })
            .unwrap();
        let graph = IterationCaptureGraph {
            nodes: vec![IterationCaptureNode {
                closure,
                release_captures: Vec::new(),
                sources: vec![IterationCaptureSource {
                    capture: ClosureCaptureDescriptor::new(
                        closure,
                        ClosureCaptureSource::Symbol(SymbolId(0)),
                        TypeId::new(0),
                        ClosureCaptureMode::Owned,
                        ClosureCaptureEffect::Move,
                        span,
                    ),
                    position: 0,
                    captured: Vec::new(),
                    may_be_opaque: false,
                }],
            }],
        };
        let mut conditions = CleanupConditions::default();
        let first = conditions.create_owner(CleanupOwnerValue::Closure {
            expression: closure,
            origin: span,
            inputs: Vec::new(),
        });
        let second = conditions.create_owner(CleanupOwnerValue::Closure {
            expression: closure,
            origin: span,
            inputs: Vec::new(),
        });
        let (selector, _) = conditions.last_capture_loan(first, span);
        let origin = IterationPhiIncomingOrigin {
            node: 0,
            target: selector,
            condition: CleanupConditionId::ALWAYS,
            environments: [first, second]
                .into_iter()
                .map(|root| IterationPhiIncomingEnvironment {
                    owner: root,
                    instance_root: root,
                    capture_path: Vec::new(),
                    condition: CleanupConditionId::ALWAYS,
                    sources: Vec::new(),
                })
                .collect(),
        };
        assert_eq!(
            coexisting_capture_node(&mut conditions, &graph, &[origin], &mut BTreeMap::new()),
            Some(0),
            "two distinct root instances cannot share one capture layout"
        );
    }

    #[test]
    fn owned_cycle_release_does_not_claim_independent_leaf_roots() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("owned-cycle-roots.ko", "fun run() { val f = move {} }")
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        let (closure, span) = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                matches!(node.payload(), Expression::Lambda { .. }).then_some((id, node.span()))
            })
            .unwrap();
        let capture = ClosureCaptureDescriptor::new(
            closure,
            ClosureCaptureSource::Symbol(SymbolId(0)),
            TypeId::new(0),
            ClosureCaptureMode::Owned,
            ClosureCaptureEffect::Move,
            span,
        );
        let node = |children: &[usize]| super::PhiCaptureNode {
            closure,
            release_captures: Vec::new(),
            opaque_sources: BTreeSet::new(),
            sources: children
                .iter()
                .enumerate()
                .map(|(position, child)| (capture, position, vec![*child]))
                .collect(),
        };
        // 0 <-> 1 is the recursive chain. 2 is its terminal leaf; 3 owns
        // both that chain and an independent leaf 4. Only roots 0, 1, 3
        // require instance traversal; leaves keep ordinary capture cleanup.
        let graph = PhiCaptureGraph {
            nodes: vec![
                node(&[1, 2]),
                node(&[0]),
                node(&[]),
                node(&[0, 4]),
                node(&[]),
            ],
            by_closure: BTreeMap::new(),
        };
        assert_eq!(
            graph.owned_nodes_reaching_cycle(),
            [true, true, false, true, false]
        );
    }

    #[test]
    fn nested_phi_selector_writes_keep_local_instance_paths() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("nested-phi-selector.ko", "fun run() { val f = move {} }")
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        let span = parsed.ast().expressions().iter().next().unwrap().1.span();
        let mut table = CleanupConditions::default();
        let closure = parsed.ast().expressions().iter().next().unwrap().0;
        let owner = table.create_owner(CleanupOwnerValue::Closure {
            expression: closure,
            origin: span,
            inputs: Vec::new(),
        });
        let (first, _) = table.last_capture_loan(owner, span);
        let (second, _) = table.last_capture_loan(owner, span);
        let (child_selector, _) = table.last_capture_loan(owner, span);
        let (missing_selector, _) = table.last_capture_loan(owner, span);
        let layout = [first, second, child_selector, missing_selector]
            .into_iter()
            .enumerate()
            .map(|(node, selector)| IterationClosurePhiOrigin {
                node,
                closure,
                selector,
                condition: CleanupConditionId::ALWAYS,
                sources: Vec::new(),
            })
            .collect::<Vec<_>>();
        let capture_source = ClosureCaptureSource::Symbol(SymbolId(0));
        let input = CleanupCaptureInput {
            source: capture_source,
            value: CleanupCaptureValue::Place(capture_source),
            mode: ClosureCaptureMode::Owned,
            effect: ClosureCaptureEffect::Move,
            condition: CleanupConditionId::ALWAYS,
            origin: span,
        };
        let root =
            |node, target, child_condition, child_path: Option<usize>| IterationPhiIncomingOrigin {
                node,
                target,
                condition: CleanupConditionId::ALWAYS,
                environments: vec![IterationPhiIncomingEnvironment {
                    owner,
                    instance_root: owner,
                    capture_path: Vec::new(),
                    condition: CleanupConditionId::ALWAYS,
                    sources: vec![IterationPhiIncomingSource {
                        target: None,
                        capture_slot: None,
                        source_capture_slot: None,
                        read_address: None,
                        source_environment: owner,
                        nested_instance: false,
                        input,
                        captured: vec![IterationPhiIncomingOrigin {
                            node: 2,
                            target: child_selector,
                            condition: child_condition,
                            environments: child_path
                                .into_iter()
                                .map(|position| IterationPhiIncomingEnvironment {
                                    owner,
                                    instance_root: owner,
                                    capture_path: vec![position],
                                    condition: CleanupConditionId::ALWAYS,
                                    sources: Vec::new(),
                                })
                                .collect(),
                        }],
                    }],
                }],
            };
        let origins = vec![
            root(0, first, CleanupConditionId::NEVER, None),
            root(1, second, CleanupConditionId::ALWAYS, None),
        ];
        let writes = phi_selector_writes(&mut table, &layout, &origins, &[]);
        assert_eq!(writes.len(), layout.len());
        assert_eq!(writes[2].condition(), CleanupConditionId::ALWAYS);
        assert_eq!(writes[3].condition(), CleanupConditionId::NEVER);
        assert_eq!(
            origins[0].environments()[0].sources()[0].captured()[0].condition(),
            CleanupConditionId::NEVER
        );

        let distinct_paths = vec![
            root(0, first, CleanupConditionId::ALWAYS, Some(0)),
            root(1, second, CleanupConditionId::ALWAYS, Some(1)),
        ];
        let writes = phi_selector_writes(&mut table, &layout, &distinct_paths, &[]);
        let child_paths = distinct_paths
            .iter()
            .map(|origin| {
                origin.environments()[0].sources()[0].captured()[0]
                    .environments()
                    .iter()
                    .map(|environment| environment.capture_path().to_vec())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(child_paths, [vec![vec![0]], vec![vec![1]]]);
        assert_eq!(writes[2].condition(), CleanupConditionId::ALWAYS);
    }

    #[test]
    fn conditional_leaf_phi_replays_formed_instance_and_presence() {
        for tail in ["", "continue", "break"] {
            assert_conditional_leaf_replay(tail);
        }
    }

    fn assert_conditional_leaf_replay(tail: &str) {
        fn mentions(
            table: &CleanupConditions,
            condition: CleanupConditionId,
            wanted: crate::ownership_checking::CleanupSelectorId,
        ) -> bool {
            match table.get(condition).unwrap() {
                CleanupCondition::Choice { selector, branches } => {
                    *selector == wanted
                        || branches
                            .iter()
                            .any(|branch| mentions(table, *branch, wanted))
                }
                CleanupCondition::Always | CleanupCondition::Never => false,
            }
        }

        fn replay_presence(
            table: &CleanupConditions,
            binding: &IterationPhiIncomingBinding,
            choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
        ) {
            let before = choices.clone();
            let mut writes = vec![(
                binding.availability_selector(),
                usize::from(selected(table, binding.available_when(), &before)),
            )];
            for write in binding.selector_writes() {
                writes.push((
                    write.target(),
                    usize::from(selected(table, write.condition(), &before)),
                ));
            }
            for (target, value) in writes {
                choices.insert(target, value);
            }
        }

        struct EnvironmentInstance {
            closure: ExpressionId,
            choices: BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
            captured: BTreeMap<usize, usize>,
        }

        #[derive(Default)]
        struct ReplayState {
            next_instance: usize,
            instances: BTreeMap<usize, EnvironmentInstance>,
            owners: BTreeMap<CleanupOwnerValueId, usize>,
            phi_slots: BTreeMap<crate::ownership_checking::CleanupCaptureSlotId, usize>,
            choices: BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
        }

        impl ReplayState {
            fn pass_closure(
                &self,
                steps: &[(DropPoint, IterationCleanupAction)],
                call: ExpressionId,
            ) -> (ExpressionId, usize) {
                let actions = steps
                    .iter()
                    .filter(|(point, _)| *point == DropPoint::CallEntry(call))
                    .map(|(_, action)| action)
                    .collect::<Vec<_>>();
                let [action] = actions.as_slice() else {
                    panic!("call entry must select exactly one concrete closure environment")
                };
                let IterationCleanupAction::PassClosureEnvironment { callee, closure } = **action
                else {
                    unreachable!()
                };
                (
                    closure.expect("this fixture has a unique concrete lambda"),
                    self.owners[&callee],
                )
            }

            fn enter_closure(
                &mut self,
                steps: &[(DropPoint, IterationCleanupAction)],
                closure: ExpressionId,
                instance: usize,
            ) {
                let mut actions = steps
                    .iter()
                    .filter(|(point, _)| *point == DropPoint::LambdaEntry(closure))
                    .map(|(_, action)| action);
                let Some(IterationCleanupAction::BindClosureEnvironment {
                    owner,
                    closure: entered,
                }) = actions.next()
                else {
                    panic!("lambda entry must bind its incoming environment first")
                };
                assert_eq!(*entered, closure);
                assert!(self.instances.contains_key(&instance));
                assert!(self.owners.insert(*owner, instance).is_none());
                assert!(!actions.any(|action| matches!(
                    action,
                    IterationCleanupAction::BindClosureEnvironment { .. }
                )));
            }

            fn replay_point(
                &mut self,
                table: &CleanupConditions,
                steps: &[(DropPoint, IterationCleanupAction)],
                point: DropPoint,
            ) -> usize {
                let mut formation_actions = 0;
                for (_, action) in steps.iter().filter(|(at, _)| *at == point) {
                    match action {
                        IterationCleanupAction::CreateClosureOwner { .. } => {
                            self.create(action);
                            formation_actions += 1;
                        }
                        IterationCleanupAction::SaveClosureCapture { .. } => {
                            self.capture(table, action);
                            formation_actions += 1;
                        }
                        IterationCleanupAction::SaveOwnerSnapshot { .. } => {
                            self.snapshot(table, action);
                            formation_actions += 1;
                        }
                        _ => {}
                    }
                }
                formation_actions
            }

            fn create(&mut self, action: &IterationCleanupAction) -> usize {
                let IterationCleanupAction::CreateClosureOwner { owner, closure } = action else {
                    panic!("formation must start with CreateClosureOwner")
                };
                self.next_instance += 1;
                self.instances.insert(
                    self.next_instance,
                    EnvironmentInstance {
                        closure: *closure,
                        choices: BTreeMap::new(),
                        captured: BTreeMap::new(),
                    },
                );
                assert!(self.owners.insert(*owner, self.next_instance).is_none());
                self.next_instance
            }

            fn capture(&mut self, table: &CleanupConditions, action: &IterationCleanupAction) {
                let IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } = action
                else {
                    panic!("capture must use SaveClosureCapture")
                };
                assert!(selected(table, input.condition(), &self.choices));
                assert_eq!(input.effect(), ClosureCaptureEffect::Move);
                let source = match input.value() {
                    CleanupCaptureValue::Owner(owner) => self.owners.remove(&owner).unwrap(),
                    CleanupCaptureValue::Environment { owner, slot, .. } => {
                        let environment = self.owners[&owner];
                        let position = table.capture_slot_value(slot).unwrap().position();
                        self.instances
                            .get_mut(&environment)
                            .unwrap()
                            .captured
                            .remove(&position)
                            .unwrap()
                    }
                    CleanupCaptureValue::Place(_) => panic!("owned capture needs an instance"),
                };
                let environment = self.owners[owner];
                let position = table.capture_slot_value(*target).unwrap().position();
                assert!(
                    self.instances
                        .get_mut(&environment)
                        .unwrap()
                        .captured
                        .insert(position, source)
                        .is_none()
                );
            }

            fn snapshot(&mut self, table: &CleanupConditions, action: &IterationCleanupAction) {
                let IterationCleanupAction::SaveOwnerSnapshot {
                    condition,
                    owner,
                    value,
                } = action
                else {
                    panic!("snapshot must use SaveOwnerSnapshot")
                };
                assert!(condition.is_none_or(|guard| selected(table, guard, &self.choices)));
                let snapshot = table.owner_snapshot(*owner).unwrap();
                assert_eq!(snapshot.value(), *value);
                let inputs = snapshot
                    .capture_inputs()
                    .iter()
                    .filter(|input| selected(table, input.condition(), &self.choices))
                    .collect::<Vec<_>>();
                assert_eq!(inputs.len(), 1);
                let instance = self.owners.remove(&inputs[0].owner()).unwrap();
                let before = self.choices.clone();
                let mut copies = Vec::new();
                for copy in snapshot.copies() {
                    if !selected(table, copy.when(), &before) {
                        continue;
                    }
                    let value = match copy.source_value() {
                        Some(CleanupCaptureValue::Owner(owner)) => {
                            self.instances[&self.owners[&owner]].choices[&copy.source()]
                        }
                        Some(CleanupCaptureValue::Environment { owner, slot, .. }) => {
                            let parent = if owner == inputs[0].owner() {
                                instance
                            } else {
                                self.owners[&owner]
                            };
                            let position = table.capture_slot_value(slot).unwrap().position();
                            let child = self.instances[&parent].captured[&position];
                            self.instances[&child].choices[&copy.source()]
                        }
                        Some(CleanupCaptureValue::Place(_)) => {
                            panic!("snapshot selector cannot come from a place")
                        }
                        None => self.instances[&instance]
                            .choices
                            .get(&copy.source())
                            .or_else(|| before.get(&copy.source()))
                            .copied()
                            .expect("snapshot must read a formed selector"),
                    };
                    copies.push((copy.target(), value));
                }
                for (target, value) in copies {
                    self.instances
                        .get_mut(&instance)
                        .unwrap()
                        .choices
                        .insert(target, value);
                    self.choices.insert(target, value);
                }
                assert!(self.owners.insert(*owner, instance).is_none());
            }

            fn copy_phi(
                &mut self,
                table: &CleanupConditions,
                graph: &IterationCaptureGraph,
                edge: &crate::ownership_checking::IterationPhiIncoming,
                binding: &IterationPhiIncomingBinding,
            ) {
                let before = self.choices.clone();
                assert!(selected(table, edge.condition(), &before));
                let available = selected(table, binding.available_when(), &before);
                let values = binding
                    .values()
                    .iter()
                    .filter(|value| selected(table, value.condition(), &before))
                    .collect::<Vec<_>>();
                assert_eq!(values.len(), usize::from(available));
                let roots = binding
                    .root_sources()
                    .iter()
                    .filter(|root| selected(table, root.condition(), &before))
                    .collect::<Vec<_>>();
                assert_eq!(roots.len(), values.len());
                let old_owners = self.owners.clone();
                let mut source_writes = Vec::new();
                for origin in binding.origins() {
                    if !selected(table, origin.condition(), &before) {
                        continue;
                    }
                    for environment in origin
                        .environments()
                        .iter()
                        .filter(|environment| selected(table, environment.condition(), &before))
                    {
                        for source in environment
                            .sources()
                            .iter()
                            .filter(|source| selected(table, source.input().condition(), &before))
                        {
                            if let Some(target) = source.capture_slot() {
                                source_writes.push((
                                    target,
                                    read_captured(table, source, &old_owners, &self.instances),
                                ));
                            }
                        }
                    }
                }
                let root_write = roots.first().map(|root| {
                    assert_eq!(root.source(), values[0].source());
                    let instance = old_owners[&root.source()];
                    assert_eq!(
                        graph.nodes()[root.node()].closure(),
                        self.instances[&instance].closure
                    );
                    (root.source(), instance)
                });
                let mut new_choices = before;
                replay_presence(table, binding, &mut new_choices);
                for &slot in binding.capture_slots_to_clear() {
                    self.phi_slots.remove(&slot);
                }
                for (slot, instance) in source_writes {
                    assert!(self.phi_slots.insert(slot, instance).is_none());
                }
                if let Some((source, instance)) = root_write {
                    if source != binding.target() {
                        self.owners.remove(&source).unwrap();
                    }
                    self.owners.insert(binding.target(), instance);
                } else {
                    self.owners.remove(&binding.target());
                }
                self.choices = new_choices;
            }
        }

        fn read_captured(
            table: &CleanupConditions,
            source: &IterationPhiIncomingSource,
            owners: &BTreeMap<CleanupOwnerValueId, usize>,
            instances: &BTreeMap<usize, EnvironmentInstance>,
        ) -> usize {
            let (address, slot) = source.transport_read().unwrap();
            let address = table.instance_address(address).unwrap();
            let mut instance = owners[&address.root()];
            for &position in address.capture_path() {
                instance = instances[&instance].captured[&position];
            }
            let position = table.capture_slot_value(slot).unwrap().position();
            instances[&instance].captured[&position]
        }

        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "conditional-leaf.ko",
                format!("fun run(flag: Boolean) {{ val base: move () -> Unit = if (flag) (move {{}}) else (move {{}})\nval outer: move () -> Unit = move {{ var f: move () -> Unit = move {{ base() }}\nfor (_ in listOf(1, 2)) {{ {tail} }}\nval used = f() }}\nval used = outer() }}"),
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.enclosing_capture_phi.is_some());
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let crate::parser::Statement::For { source, body, .. } =
            parsed.ast().statements().get(statement).unwrap().payload()
        else {
            panic!("expected for loop");
        };
        let entry = planner.loop_phi_incomings[&statement.index()]
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
        assert_eq!(entry.point(), DropPoint::AfterExpression(*source));
        assert_eq!(entry.boundary(), IterationPhiBoundary::Header);
        let parent = entry
            .bindings()
            .iter()
            .flat_map(|binding| binding.origins())
            .flat_map(|origin| origin.environments())
            .find(|environment| {
                environment
                    .sources()
                    .iter()
                    .any(|source| !source.captured().is_empty())
            })
            .unwrap();
        let snapshot = planner.conditions.owner_snapshot(parent.owner()).unwrap();
        let copy = snapshot
            .copies()
            .iter()
            .find(|copy| copy.source_value().is_some())
            .unwrap();
        let nested = parent
            .sources()
            .iter()
            .flat_map(|source| source.captured())
            .collect::<Vec<_>>();
        assert_eq!(nested.len(), 2);
        for origin in &nested {
            assert!(mentions(
                &planner.conditions,
                origin.condition(),
                copy.target()
            ));
        }
        let source = parent
            .sources()
            .iter()
            .find(|source| !source.captured().is_empty())
            .unwrap();
        assert!(source.transport_value().is_some());
        let (address, slot) = source.transport_read().unwrap();
        let address = planner.conditions.instance_address(address).unwrap();
        assert_eq!(address.root(), parent.owner());
        assert!(address.capture_path().is_empty());
        let source_slot = planner.conditions.capture_slot_value(slot).unwrap();
        assert_ne!(source_slot.environment(), parent.owner());
        assert!(matches!(
            planner.conditions.owner_value(source_slot.environment()),
            Some(CleanupOwnerValue::Closure { .. })
        ));
        assert!(matches!(
            planner.conditions.owner_value(parent.owner()),
            Some(CleanupOwnerValue::Snapshot(_))
        ));
        assert_eq!(source_slot.source(), source.input().source());
        assert_eq!(source_slot.position(), 0);
        let header = planner.loop_phis[&statement.index()]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
            .unwrap();
        let entry_binding = entry
            .bindings()
            .iter()
            .find(|binding| binding.target() == header.owner())
            .unwrap();
        assert_eq!(entry_binding.values().len(), 1);
        assert_eq!(entry_binding.values()[0].source(), parent.owner());
        let jump_edge = planner.loop_phi_incomings[&statement.index()]
            .iter()
            .find(|incoming| match (tail, incoming.kind()) {
                ("continue", IterationPhiIncomingKind::Continue(_))
                | ("break", IterationPhiIncomingKind::Break(_))
                | ("", IterationPhiIncomingKind::Fallthrough) => true,
                _ => false,
            })
            .unwrap();
        let jump_target = if tail == "break" {
            IterationPhiBoundary::Exit
        } else {
            IterationPhiBoundary::Header
        };
        assert_eq!(jump_edge.boundary(), jump_target);
        match jump_edge.point() {
            DropPoint::AfterStatement(completed) if tail.is_empty() && completed == *body => {}
            DropPoint::ControlTransfer(control) if !tail.is_empty() => {
                assert_eq!(
                    sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
                    Ok(tail)
                );
            }
            other => panic!("{tail} phi input has the wrong execution point: {other:?}"),
        }
        let carried = jump_edge
            .bindings()
            .iter()
            .find(|binding| {
                planner.loop_phis[&statement.index()].iter().any(|phi| {
                    phi.boundary() == jump_target
                        && phi.symbol() == header.symbol()
                        && phi.owner() == binding.target()
                })
            })
            .unwrap();
        assert_eq!(carried.values().len(), 1);
        assert_eq!(carried.values()[0].source(), header.owner());
        let forwarded = carried
            .origins()
            .iter()
            .flat_map(|origin| origin.environments())
            .find(|environment| environment.owner() == header.owner())
            .unwrap();
        assert!(
            planner
                .conditions
                .owner_snapshot(forwarded.owner())
                .is_none()
        );
        let child_selectors = header
            .origins()
            .iter()
            .flat_map(|origin| origin.sources())
            .flat_map(|source| source.captured())
            .map(|&index| header.origins()[index].selector())
            .collect::<Vec<_>>();
        assert_eq!(child_selectors.len(), 2);
        assert!(child_selectors.iter().all(|selector| matches!(
            planner.conditions.selector(*selector).unwrap().source(),
            CleanupSelectorSource::IterationPhi {
                boundary: IterationPhiBoundary::Header,
                ..
            }
        )));
        let forwarded_children = forwarded
            .sources()
            .iter()
            .flat_map(|source| source.captured())
            .collect::<Vec<_>>();
        assert_eq!(forwarded_children.len(), 2);
        for child in &forwarded_children {
            assert!(!mentions(
                &planner.conditions,
                child.condition(),
                copy.target()
            ));
            assert!(child_selectors.iter().any(|&selector| mentions(
                &planner.conditions,
                child.condition(),
                selector
            )));
        }
        let exhaustion = planner.loop_phi_incomings[&statement.index()]
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
            .unwrap();
        assert_eq!(exhaustion.point(), DropPoint::LoopExit(statement));
        assert_eq!(exhaustion.boundary(), IterationPhiBoundary::Exit);
        let exit = planner.loop_phis[&statement.index()]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
            .unwrap();
        let exit_binding = exhaustion
            .bindings()
            .iter()
            .find(|binding| binding.target() == exit.owner())
            .unwrap();
        assert_eq!(exit_binding.values().len(), 1);
        assert_eq!(exit_binding.values()[0].source(), header.owner());
        let exit_environment = exit_binding
            .origins()
            .iter()
            .flat_map(|origin| origin.environments())
            .find(|environment| environment.owner() == header.owner())
            .unwrap();
        assert_eq!(exit_environment.instance_root(), header.owner());
        let exit_source = exit_environment
            .sources()
            .iter()
            .find(|source| !source.captured().is_empty())
            .unwrap();
        let (exit_address, exit_slot) = exit_source.transport_read().unwrap();
        let exit_address = planner.conditions.instance_address(exit_address).unwrap();
        assert_eq!(exit_address.root(), header.owner());
        assert!(exit_address.capture_path().is_empty());
        let exit_slot = planner.conditions.capture_slot_value(exit_slot).unwrap();
        assert_eq!(exit_slot.environment(), header.owner());
        assert_eq!(exit_slot.source(), exit_source.input().source());
        assert_eq!(exit_slot.position(), 0);
        let exit_children = exit_binding
            .origins()
            .iter()
            .flat_map(|origin| origin.environments())
            .flat_map(|environment| environment.sources())
            .flat_map(|source| source.captured())
            .collect::<Vec<_>>();
        assert_eq!(exit_children.len(), 2);
        let base_snapshot = planner
            .cleanup
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, .. } => {
                    planner.conditions.owner_snapshot(*owner)
                }
                _ => None,
            })
            .find(|snapshot| {
                snapshot
                    .copies()
                    .iter()
                    .any(|candidate| candidate.target() == copy.source())
            })
            .unwrap();
        let base_copy = base_snapshot
            .copies()
            .iter()
            .find(|candidate| candidate.target() == copy.source())
            .unwrap();
        let (outer_owner, outer_slot) = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if input.value() == CleanupCaptureValue::Owner(base_snapshot.owner()) => {
                    Some((*owner, *target))
                }
                _ => None,
            })
            .unwrap();
        let (inner_owner, inner_slot) = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if matches!(
                    input.value(),
                    CleanupCaptureValue::Environment { owner: source, slot, .. }
                        if source == outer_owner && slot == outer_slot
                ) =>
                {
                    Some((*owner, *target))
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(inner_owner, source_slot.environment());
        assert_eq!(snapshot.capture_inputs().len(), 1);
        assert_eq!(snapshot.capture_inputs()[0].owner(), inner_owner);
        assert_eq!(
            copy.source_value(),
            Some(CleanupCaptureValue::Environment {
                owner: inner_owner,
                source: source.input().source(),
                slot: inner_slot,
            })
        );
        let outer_position = planner
            .conditions
            .capture_slot_value(outer_slot)
            .unwrap()
            .position();
        let inner_position = planner
            .conditions
            .capture_slot_value(inner_slot)
            .unwrap()
            .position();
        let graph = &planner.loop_capture_graphs[&statement.index()];
        let f_call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()) == Ok("f()")).then_some(id))
            .unwrap();
        let call_drops = planner
            .cleanup
            .iter()
            .filter(|(point, _)| *point == DropPoint::CallReturn(f_call))
            .map(|(_, action)| *action)
            .collect::<Vec<_>>();
        let [
            IterationCleanupAction::Drop(child_drop),
            IterationCleanupAction::Drop(root_drop),
        ] = call_drops.as_slice()
        else {
            panic!("f() must release its captured leaf before its environment: {call_drops:?}")
        };
        let DropTarget::Captured {
            closure: released_closure,
            source: released_source,
            ..
        } = child_drop.target()
        else {
            panic!("the first f() cleanup must release the captured leaf")
        };
        assert_eq!(root_drop.target(), DropTarget::Named(exit.symbol()));
        assert_eq!(
            sources.slice(names.symbols()[exit.symbol().index()].span()),
            Ok("f")
        );
        assert_eq!(child_drop.owner(), None);
        assert_eq!(root_drop.owner(), Some(exit.owner()));
        let child_address = planner
            .conditions
            .instance_address(child_drop.instance_address().unwrap())
            .unwrap();
        assert_eq!(child_address.root(), exit.owner());
        assert!(child_address.capture_path().is_empty());
        let child_slot = planner
            .conditions
            .capture_slot_value(child_drop.capture_slot().unwrap())
            .unwrap();
        let formed_slot = planner.conditions.capture_slot_value(inner_slot).unwrap();
        assert_eq!(child_slot.environment(), exit.owner());
        assert_eq!(child_slot.closure(), released_closure);
        assert_eq!(child_slot.source(), released_source);
        assert_eq!(child_slot.closure(), formed_slot.closure());
        assert_eq!(child_slot.source(), formed_slot.source());
        assert_eq!(child_slot.position(), inner_position);
        let creation = |wanted| {
            planner
                .cleanup
                .iter()
                .enumerate()
                .find_map(|(index, (_, action))| match action {
                    IterationCleanupAction::CreateClosureOwner { owner, .. }
                        if *owner == wanted =>
                    {
                        Some((index, *action))
                    }
                    _ => None,
                })
                .unwrap()
        };
        let snapshot_action = |wanted| {
            planner
                .cleanup
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::SaveOwnerSnapshot { owner, .. } if *owner == wanted => {
                        Some(*action)
                    }
                    _ => None,
                })
                .unwrap()
        };
        let (outer_create_index, outer_create) = creation(outer_owner);
        let IterationCleanupAction::CreateClosureOwner {
            closure: outer_closure,
            ..
        } = outer_create
        else {
            unreachable!()
        };
        let outer_call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()) == Ok("outer()")).then_some(id))
            .unwrap();
        let (inner_create_index, _) = creation(inner_owner);
        let (outer_capture_index, _) = planner
            .cleanup
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveClosureCapture { owner, target, .. }
                    if *owner == outer_owner && *target == outer_slot =>
                {
                    Some((index, *action))
                }
                _ => None,
            })
            .unwrap();
        let (inner_capture_index, _) = planner
            .cleanup
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveClosureCapture { owner, target, .. }
                    if *owner == inner_owner && *target == inner_slot =>
                {
                    Some((index, *action))
                }
                _ => None,
            })
            .unwrap();
        assert!(outer_create_index < outer_capture_index);
        assert!(inner_create_index < inner_capture_index);
        assert_eq!(
            planner.cleanup[outer_create_index].0,
            planner.cleanup[outer_capture_index].0
        );
        assert_eq!(
            planner.cleanup[inner_create_index].0,
            planner.cleanup[inner_capture_index].0
        );
        let base_save = snapshot_action(base_snapshot.owner());
        let inner_save = snapshot_action(parent.owner());
        let base_point = planner
            .cleanup
            .iter()
            .find(|(_, action)| *action == base_save)
            .unwrap()
            .0;
        let outer_point = planner.cleanup[outer_create_index].0;
        let inner_point = planner.cleanup[inner_create_index].0;
        let f_statement = parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| {
                (matches!(
                    node.payload(),
                    crate::parser::Statement::LocalVariable { .. }
                ) && sources
                    .slice(node.span())
                    .is_ok_and(|text| text.starts_with("var f:")))
                .then_some(id)
            })
            .unwrap();
        let outer_snapshot_owner = planner
            .cleanup
            .iter()
            .find_map(|(point, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, .. }
                    if *point == outer_point
                        && planner
                            .conditions
                            .owner_snapshot(*owner)
                            .is_some_and(|snapshot| {
                                snapshot
                                    .capture_inputs()
                                    .iter()
                                    .any(|input| input.owner() == outer_owner)
                            }) =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        assert!(planner.cleanup.iter().any(|(point, action)| {
            *point == DropPoint::CallEntry(outer_call)
                && matches!(
                    action,
                    IterationCleanupAction::PassClosureEnvironment { callee, closure }
                        if *callee == outer_snapshot_owner && *closure == Some(outer_closure)
                )
        }));
        let inner_save_index = planner
            .cleanup
            .iter()
            .position(|(_, action)| *action == inner_save)
            .unwrap();
        assert!(inner_capture_index < inner_save_index);
        let mut next_instance = 0;
        let mut prior_inner = None;
        for arm in 0..2 {
            let control = BTreeMap::from([(base_copy.source(), arm)]);
            let branch = base_snapshot
                .capture_inputs()
                .iter()
                .filter(|input| selected(&planner.conditions, input.condition(), &control))
                .collect::<Vec<_>>();
            assert_eq!(branch.len(), 1);
            let branch_owner = branch[0].owner();
            let Some(CleanupOwnerValue::Closure {
                expression: branch_closure,
                ..
            }) = planner.conditions.owner_value(branch_owner)
            else {
                panic!("selected base branch must form a closure")
            };
            let (branch_index, branch_create) = creation(branch_owner);
            assert!(
                planner
                    .cleanup
                    .iter()
                    .enumerate()
                    .any(|(index, (_, action))| { index > branch_index && *action == base_save })
            );
            let branch_point = planner.cleanup[branch_index].0;
            let (
                DropPoint::AfterExpression(branch_expression),
                DropPoint::AfterExpression(base_expression),
                DropPoint::AfterExpression(outer_expression),
                DropPoint::AfterExpression(inner_expression),
            ) = (branch_point, base_point, outer_point, inner_point)
            else {
                panic!("formation actions must be attached to completed expressions")
            };
            assert_eq!(branch_expression, *branch_closure);
            assert_eq!(base_expression, base_snapshot.value());
            let branch_span = parsed
                .ast()
                .expressions()
                .get(branch_expression)
                .unwrap()
                .span();
            let base_span = parsed
                .ast()
                .expressions()
                .get(base_expression)
                .unwrap()
                .span();
            let outer_span = parsed
                .ast()
                .expressions()
                .get(outer_expression)
                .unwrap()
                .span();
            let inner_span = parsed
                .ast()
                .expressions()
                .get(inner_expression)
                .unwrap()
                .span();
            assert!(
                base_span.start() <= branch_span.start() && branch_span.end() <= base_span.end()
            );
            assert!(base_span.end() <= outer_span.start());
            assert!(
                outer_span.start() <= inner_span.start() && inner_span.end() <= outer_span.end()
            );
            assert!(matches!(
                branch_create,
                IterationCleanupAction::CreateClosureOwner { .. }
            ));
            let mut replay = ReplayState {
                next_instance,
                choices: control,
                ..ReplayState::default()
            };
            assert_eq!(
                replay.replay_point(&planner.conditions, &planner.cleanup, branch_point),
                1
            );
            let base_instance = replay.owners[&branch_owner];
            assert_eq!(
                replay.replay_point(&planner.conditions, &planner.cleanup, base_point),
                1
            );
            assert_eq!(
                replay.replay_point(&planner.conditions, &planner.cleanup, outer_point),
                3
            );
            let outer_instance = replay.owners[&outer_snapshot_owner];
            assert_eq!(
                replay.instances[&outer_instance].captured[&outer_position],
                base_instance
            );
            // 调用点按当前 snapshot owner 取实例，入口再绑定 body 环境 owner。
            let (entered, incoming) = replay.pass_closure(&planner.cleanup, outer_call);
            assert_eq!(entered, outer_closure);
            assert_eq!(incoming, outer_instance);
            replay.enter_closure(&planner.cleanup, entered, incoming);
            assert_eq!(replay.owners[&outer_owner], outer_instance);
            replay.choices.remove(&base_copy.source());
            replay.choices.remove(&base_copy.target());
            assert_eq!(
                replay.replay_point(&planner.conditions, &planner.cleanup, inner_point),
                3
            );
            let inner_instance = replay.owners[&parent.owner()];
            if let Some(prior) = prior_inner.replace(inner_instance) {
                assert_ne!(
                    prior, inner_instance,
                    "the same static lambda forms a new instance"
                );
            }
            assert!(
                !replay.instances[&outer_instance]
                    .captured
                    .contains_key(&outer_position)
            );
            assert_eq!(
                replay.instances[&inner_instance].captured[&inner_position],
                base_instance
            );
            assert_eq!(replay.owners[&parent.owner()], inner_instance);
            assert_eq!(
                replay.instances[&inner_instance].choices[&copy.target()],
                arm
            );
            next_instance = replay.next_instance;
            let entry_presence = nested
                .iter()
                .map(|origin| selected(&planner.conditions, origin.condition(), &replay.choices))
                .collect::<Vec<_>>();
            assert_eq!(entry_presence.iter().filter(|&&present| present).count(), 1);
            let selected_leaf = nested
                .iter()
                .find(|origin| selected(&planner.conditions, origin.condition(), &replay.choices))
                .unwrap();
            assert_eq!(
                graph.nodes()[selected_leaf.node()].closure(),
                *branch_closure
            );
            assert_eq!(selected_leaf.environments().len(), 1);
            assert_eq!(
                selected_leaf.environments()[0].instance_root(),
                parent.owner()
            );
            assert_eq!(
                selected_leaf.environments()[0].capture_path(),
                &[inner_position]
            );
            assert_eq!(
                read_captured(
                    &planner.conditions,
                    source,
                    &replay.owners,
                    &replay.instances,
                ),
                base_instance
            );
            let assert_no_early_release = |point: DropPoint,
                                           choices: &BTreeMap<
                crate::ownership_checking::CleanupSelectorId,
                usize,
            >| {
                for (_, action) in planner.cleanup.iter().filter(|(at, _)| *at == point) {
                    let fact = match action {
                        IterationCleanupAction::Drop(fact)
                        | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => {
                            fact
                        }
                        _ => continue,
                    };
                    if !fact
                        .condition()
                        .is_none_or(|guard| selected(&planner.conditions, guard, choices))
                    {
                        continue;
                    }
                    let address_root = fact.instance_address().map(|address| {
                        planner.conditions.instance_address(address).unwrap().root()
                    });
                    assert!(
                        ![branch_owner, parent.owner(), header.owner(), exit.owner()]
                            .into_iter()
                            .any(|owner| fact.owner() == Some(owner) || address_root == Some(owner))
                            && !matches!(fact.target(), DropTarget::Named(symbol) if symbol == header.symbol())
                            && !matches!(fact.target(), DropTarget::Captured { closure, .. } if closure == inner_expression),
                        "{tail}: active f or its leaf released at {point:?}: {fact:?}"
                    );
                }
            };
            assert_no_early_release(inner_point, &replay.choices);
            assert_no_early_release(DropPoint::AfterStatement(f_statement), &replay.choices);
            assert_no_early_release(entry.point(), &replay.choices);
            replay.copy_phi(&planner.conditions, graph, entry, entry_binding);
            assert!(!replay.owners.contains_key(&parent.owner()));
            assert_eq!(replay.owners[&header.owner()], inner_instance);
            let header_slot = source.capture_slot().unwrap();
            assert!(
                entry_binding
                    .capture_slots_to_clear()
                    .contains(&header_slot)
            );
            assert_eq!(replay.phi_slots[&header_slot], base_instance);
            replay.choices.remove(&copy.target());
            for (origin, present) in nested.iter().zip(&entry_presence) {
                assert_eq!(replay.choices[&origin.target()], usize::from(*present));
            }
            let jump_source = forwarded
                .sources()
                .iter()
                .find(|source| !source.captured().is_empty())
                .unwrap();
            let formed_instances = replay.instances.len();
            for round in 0..if tail == "break" { 1 } else { 2 } {
                assert_no_early_release(jump_edge.point(), &replay.choices);
                let jump_presence = forwarded_children
                    .iter()
                    .map(|origin| {
                        selected(&planner.conditions, origin.condition(), &replay.choices)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(jump_presence, entry_presence, "{tail}, round {round}");
                assert_eq!(replay.owners[&header.owner()], inner_instance);
                assert_eq!(replay.instances.len(), formed_instances);
                assert_eq!(
                    read_captured(
                        &planner.conditions,
                        jump_source,
                        &replay.owners,
                        &replay.instances,
                    ),
                    base_instance
                );
                replay.copy_phi(&planner.conditions, graph, jump_edge, carried);
                if tail != "break" {
                    assert_eq!(replay.owners[&header.owner()], inner_instance);
                    assert_eq!(replay.phi_slots[&header_slot], base_instance);
                    for (origin, present) in nested.iter().zip(&entry_presence) {
                        assert_eq!(replay.choices[&origin.target()], usize::from(*present));
                    }
                }
            }
            if tail == "break" {
                assert!(!replay.owners.contains_key(&header.owner()));
                let jump_slot = jump_source.capture_slot().unwrap();
                assert!(carried.capture_slots_to_clear().contains(&jump_slot));
                assert_eq!(replay.phi_slots[&jump_slot], base_instance);
                for (origin, present) in forwarded_children.iter().zip(&entry_presence) {
                    assert_eq!(replay.choices[&origin.target()], usize::from(*present));
                }
            } else {
                assert_no_early_release(exhaustion.point(), &replay.choices);
                let exit_presence = exit_children
                    .iter()
                    .map(|origin| {
                        selected(&planner.conditions, origin.condition(), &replay.choices)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(exit_presence, entry_presence);
                assert_eq!(
                    read_captured(
                        &planner.conditions,
                        exit_source,
                        &replay.owners,
                        &replay.instances,
                    ),
                    base_instance
                );
                replay.copy_phi(&planner.conditions, graph, exhaustion, exit_binding);
                let exit_capture_slot = exit_source.capture_slot().unwrap();
                assert!(
                    exit_binding
                        .capture_slots_to_clear()
                        .contains(&exit_capture_slot)
                );
                assert_eq!(replay.phi_slots[&exit_capture_slot], base_instance);
                for (origin, present) in exit_children.iter().zip(&entry_presence) {
                    assert_eq!(replay.choices[&origin.target()], usize::from(*present));
                }
            }
            assert!(!replay.owners.contains_key(&header.owner()));
            assert_eq!(replay.owners[&exit.owner()], inner_instance);
            assert_eq!(
                replay.instances[&inner_instance].captured[&inner_position],
                base_instance
            );
            assert!(child_drop.condition().is_none_or(|guard| selected(
                &planner.conditions,
                guard,
                &replay.choices
            )));
            assert!(root_drop.condition().is_none_or(|guard| selected(
                &planner.conditions,
                guard,
                &replay.choices
            )));
            let root_instance = replay.owners.remove(&exit.owner()).unwrap();
            assert_eq!(root_instance, inner_instance);
            let child_instance = replay
                .instances
                .get_mut(&root_instance)
                .unwrap()
                .captured
                .remove(&child_slot.position())
                .unwrap();
            assert_eq!(child_instance, base_instance);
            assert_eq!(replay.instances[&child_instance].closure, *branch_closure);
            assert!(replay.instances[&root_instance].captured.is_empty());
        }
    }

    #[test]
    fn planner_enclosing_environment_drop_reaches_owned_descendant() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "enclosing-release.ko",
                "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {\nval base: move () -> Unit = move { read(xs) }\nval outer: move () -> Unit = move {\nvar f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.enclosing_capture_phi.is_some());
        let f_call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()) == Ok("f()")).then_some(id))
            .unwrap();
        let captured = planner
            .facts
            .iter()
            .filter(|fact| {
                fact.point() == DropPoint::CallReturn(f_call)
                    && matches!(fact.target(), DropTarget::Captured { .. })
            })
            .collect::<Vec<_>>();
        assert!(
            captured.len() >= 2,
            "f must release both base and its owned xs"
        );
        // This checks the planner's private intermediate facts; the public plan remains deferred.
        let mut paths = captured
            .iter()
            .map(|fact| {
                planner
                    .conditions
                    .instance_address(fact.instance_address().unwrap())
                    .unwrap()
                    .capture_path()
                    .to_vec()
            })
            .collect::<Vec<_>>();
        paths.sort();
        assert!(paths.contains(&Vec::<usize>::new()));
        assert_eq!(paths.iter().filter(|path| *path == &vec![0]).count(), 1);
    }

    #[test]
    fn enclosing_environment_keeps_opaque_parent_drop() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "enclosing-opaque.ko",
                "fun make(): move () -> Unit = move {}\nfun read(xs: List<Int>) {}\nfun run(flag: Boolean, own xs: List<Int>) {\nval base: move () -> Unit = if (flag) (move { read(xs) }) else (make())\nval outer: move () -> Unit = move {\nval inner: move () -> Unit = move { base() }\nval used = inner() }\nval used = outer() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        let inner_call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()) == Ok("inner()")).then_some(id))
            .unwrap();
        let parent = planner
            .facts
            .iter()
            .filter(|fact| {
                fact.point() == DropPoint::CallReturn(inner_call)
                    && matches!(
                        fact.target(),
                        DropTarget::Captured {
                            value: CleanupCaptureValue::Environment { .. },
                            ..
                        }
                    )
            })
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(parent.len(), 2, "known and opaque branches both own base");
        let known = parent.iter().find(|fact| fact.owner().is_some()).unwrap();
        let opaque = parent.iter().find(|fact| fact.owner().is_none()).unwrap();
        assert_eq!(known.capture_slot(), opaque.capture_slot());
        assert_eq!(known.instance_address(), opaque.instance_address());
        let overlap = planner.conditions.and(
            known.condition().unwrap_or(CleanupConditionId::ALWAYS),
            opaque.condition().unwrap_or(CleanupConditionId::ALWAYS),
        );
        assert_eq!(overlap, CleanupConditionId::NEVER);
        let covered = planner.conditions.or(
            known.condition().unwrap_or(CleanupConditionId::ALWAYS),
            opaque.condition().unwrap_or(CleanupConditionId::ALWAYS),
        );
        assert_eq!(covered, CleanupConditionId::ALWAYS);
    }

    #[test]
    fn recursive_body_formation_reads_the_current_header_instance() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-capture.ko",
                "fun run(flags: List<Int>) { var f: move () -> Unit = move {}\nfor (_ in flags) { f = move { f() } }\nval used = f() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some());
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let header = planner.loop_phis[&statement.index()]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
            .unwrap();
        let graph = &planner.loop_capture_graphs[&statement.index()];
        let roots = planner.loop_origins[&statement.index()]
            .header()
            .iter()
            .find(|binding| binding.symbol() == header.symbol())
            .unwrap()
            .origins();
        assert_eq!(
            header
                .root_nodes()
                .iter()
                .map(|&node| graph.nodes()[node].closure())
                .collect::<Vec<_>>(),
            roots
        );
        assert!(
            !header.origins().is_empty(),
            "recursive roots need finite layout nodes"
        );
        for &node in header.root_nodes() {
            assert!(header.origins().iter().any(|origin| origin.node() == node));
        }
        assert_eq!(
            header
                .origins()
                .iter()
                .map(|origin| origin.node())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            header.origins().len(),
            "a cyclic graph allocates each reachable node once"
        );
        assert!(!header.capture_layout().is_empty());
        let steps = &planner.cleanup;
        let (capture_at, environment, target, input) = steps
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if input.value() == CleanupCaptureValue::Owner(header.owner()) => {
                    Some((index, *owner, *target, *input))
                }
                _ => None,
            })
            .expect("the new environment captures the current header owner");
        assert_eq!(input.condition(), header.availability_condition());
        let capture_slot = planner.conditions.capture_slot_value(target).unwrap();
        assert_eq!(capture_slot.environment(), environment);
        assert_eq!(capture_slot.source(), input.source());
        assert_eq!(capture_slot.position(), 0);
        let create_at = steps
            .iter()
            .position(|(_, action)| matches!(
                action,
                IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == environment
            ))
            .unwrap();
        let IterationCleanupAction::CreateClosureOwner {
            closure: recursive_closure,
            ..
        } = steps[create_at].1
        else {
            unreachable!()
        };
        let (snapshot_at, snapshot) = steps
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, .. } => Some((index, *owner)),
                _ => None,
            })
            .expect("the replacement saves its value before commit");
        let (commit_at, committed_symbol) = steps
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *owner == snapshot =>
                {
                    Some((index, *target))
                }
                _ => None,
            })
            .unwrap();
        assert!(create_at < capture_at && capture_at < snapshot_at && snapshot_at < commit_at);
        assert_eq!(committed_symbol, header.symbol());
        let snapshot_input = planner
            .conditions
            .owner_snapshot(snapshot)
            .unwrap()
            .capture_inputs();
        assert_eq!(snapshot_input.len(), 1);
        assert_eq!(snapshot_input[0].owner(), environment);
        assert_eq!(snapshot_input[0].condition(), CleanupConditionId::ALWAYS);
        let incomings = &planner.loop_phi_incomings[&statement.index()];
        let entry = incomings
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
        let backedge = incomings
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
            .unwrap();
        let entry = &entry.bindings()[0];
        let backedge = &backedge.bindings()[0];
        assert_eq!(entry.target(), header.owner());
        assert_eq!(backedge.target(), header.owner());
        assert!(entry.capture_slots_to_clear().is_empty());
        assert!(backedge.capture_slots_to_clear().is_empty());
        assert_eq!(entry.values().len(), 1);
        assert_eq!(backedge.values().len(), 1);
        assert_eq!(backedge.values()[0].source(), snapshot);
        assert_eq!(entry.root_sources().len(), 1);
        assert_eq!(entry.root_sources()[0].source(), entry.values()[0].source());
        assert_eq!(backedge.root_sources().len(), 1);
        assert_eq!(backedge.root_sources()[0].source(), snapshot);
        for binding in [entry, backedge] {
            assert_eq!(
                binding.presence_source(),
                IterationPhiPresenceSource::CapturedInstances
            );
            assert!(
                binding.origins().is_empty(),
                "recursive descendants are not unfolded"
            );
            assert_eq!(binding.selector_writes().len(), header.origins().len());
            let selected_root = binding.root_sources()[0];
            let write = binding
                .selector_writes()
                .iter()
                .find(|write| write.node() == selected_root.node())
                .unwrap();
            assert_eq!(write.condition(), selected_root.condition());
        }
        for incoming in incomings.iter().filter(|incoming| {
            matches!(
                incoming.kind(),
                IterationPhiIncomingKind::Entry | IterationPhiIncomingKind::Fallthrough
            )
        }) {
            assert_eq!(incoming.condition(), CleanupConditionId::ALWAYS);
            assert_eq!(
                incoming.bindings()[0].availability_selector(),
                header.availability_selector()
            );
            assert_eq!(
                incoming.bindings()[0].available_when(),
                CleanupConditionId::ALWAYS
            );
            assert_eq!(
                incoming.bindings()[0].values()[0].condition(),
                CleanupConditionId::ALWAYS
            );
        }
        let Some(CleanupCondition::Choice { selector, branches }) =
            planner.conditions.get(input.condition())
        else {
            panic!("capture must depend on header presence")
        };
        assert_eq!(*selector, header.availability_selector());
        assert_eq!(
            branches,
            &[CleanupConditionId::NEVER, CleanupConditionId::ALWAYS]
        );

        // 同一 Create 动作重复执行必须创建不同实例，旧 header 只能进入新实例的捕获槽。
        let mut next_instance = 0;
        let mut instance_nodes = BTreeMap::new();
        let mut values = BTreeMap::new();
        let mut create = |action: IterationCleanupAction,
                          values: &mut BTreeMap<_, _>,
                          instance_nodes: &mut BTreeMap<_, _>| {
            let IterationCleanupAction::CreateClosureOwner { owner, closure } = action else {
                panic!("formation must start with CreateClosureOwner")
            };
            next_instance += 1;
            assert!(values.insert(owner, next_instance).is_none());
            let node = graph
                .nodes()
                .iter()
                .position(|node| node.closure() == closure)
                .unwrap();
            assert!(instance_nodes.insert(next_instance, node).is_none());
            next_instance
        };
        let initial_create = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, .. }
                    if *owner == entry.values()[0].source() =>
                {
                    Some(*action)
                }
                _ => None,
            })
            .unwrap();
        let seed = create(initial_create, &mut values, &mut instance_nodes);
        let mut choices = BTreeMap::new();
        let mut captured_slots = BTreeMap::new();
        let [entry_root] = entry.root_sources() else {
            panic!("entry must provide one root")
        };
        assert!(selected(
            &planner.conditions,
            entry_root.condition(),
            &choices
        ));
        let (entry_source, entry_instance) = replay_captured_edge_presence(
            &planner.conditions,
            graph,
            header,
            incomings
                .iter()
                .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
                .unwrap(),
            entry.target(),
            &values,
            &instance_nodes,
            &captured_slots,
            &mut choices,
        );
        assert_eq!(entry_source, entry_root.source());
        assert_eq!(entry_instance, seed);
        assert_eq!(values.remove(&entry_source), Some(seed));
        assert!(values.insert(entry.target(), seed).is_none());
        let zero_round = values.clone();
        let mut zero_choices = choices.clone();
        let mut rounds = Vec::new();
        for _ in 0..2 {
            let new_instance = create(steps[create_at].1, &mut values, &mut instance_nodes);
            rounds.push(new_instance);
            let IterationCleanupAction::SaveClosureCapture {
                owner,
                target: saved_slot,
                input: saved_input,
            } = steps[capture_at].1
            else {
                unreachable!()
            };
            assert_eq!(
                (owner, saved_slot, saved_input),
                (environment, target, input)
            );
            let CleanupCaptureValue::Owner(source) = saved_input.value() else {
                unreachable!()
            };
            assert_eq!(source, header.owner());
            let old = values.remove(&source).unwrap();
            assert!(
                captured_slots
                    .insert((new_instance, capture_slot.position()), old)
                    .is_none()
            );
            let IterationCleanupAction::SaveOwnerSnapshot { owner, .. } = steps[snapshot_at].1
            else {
                unreachable!()
            };
            assert_eq!(owner, snapshot);
            let created = values.remove(&snapshot_input[0].owner()).unwrap();
            values.insert(snapshot, created);
            replay_snapshot_choices(&planner.conditions, snapshot, &mut choices);
            let IterationCleanupAction::CommitOwnerSnapshot { owner, target } = steps[commit_at].1
            else {
                unreachable!()
            };
            assert_eq!((owner, target), (snapshot, committed_symbol));
            let active = backedge
                .root_sources()
                .iter()
                .filter(|root| selected(&planner.conditions, root.condition(), &choices))
                .collect::<Vec<_>>();
            assert_eq!(active.len(), 1);
            assert_eq!(active[0].source(), snapshot);
            let (source, moved) = replay_captured_edge_presence(
                &planner.conditions,
                graph,
                header,
                incomings
                    .iter()
                    .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
                    .unwrap(),
                backedge.target(),
                &values,
                &instance_nodes,
                &captured_slots,
                &mut choices,
            );
            assert_eq!(source, active[0].source());
            assert_eq!(values.remove(&source), Some(moved));
            assert!(values.insert(backedge.target(), moved).is_none());
        }
        assert_eq!(
            captured_slots,
            BTreeMap::from([((rounds[0], 0), seed), ((rounds[1], 0), rounds[0]),])
        );
        assert_eq!(values[&header.owner()], rounds[1]);
        let exit = planner.loop_phis[&statement.index()]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
            .unwrap();
        assert_eq!(exit.root_nodes(), header.root_nodes());
        let exhaustion = incomings
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
            .unwrap();
        assert_eq!(exhaustion.condition(), CleanupConditionId::ALWAYS);
        let exhausted = &exhaustion.bindings()[0];
        assert_eq!(exhausted.target(), exit.owner());
        assert!(exhausted.capture_slots_to_clear().is_empty());
        assert_eq!(
            exhausted.availability_selector(),
            exit.availability_selector()
        );
        assert_eq!(exhausted.available_when(), header.availability_condition());
        assert_eq!(exhausted.values().len(), 1);
        assert_eq!(exhausted.values()[0].source(), header.owner());
        assert_eq!(exhausted.root_sources().len(), header.root_nodes().len());
        let mut conditions = planner.conditions.clone();
        let unavailable = conditions.not(exhausted.available_when());
        for root in exhausted.root_sources() {
            assert_eq!(
                conditions.and(root.condition(), unavailable),
                CleanupConditionId::NEVER,
                "an absent header cannot provide a root handle"
            );
        }
        assert!(exhausted.origins().is_empty());
        assert_eq!(exhausted.selector_writes().len(), exit.origins().len());
        for source in exhausted.root_sources() {
            let write = exhausted
                .selector_writes()
                .iter()
                .find(|write| write.node() == source.node())
                .unwrap();
            assert_eq!(write.condition(), source.condition());
        }
        assert_eq!(
            exhausted.values()[0].condition(),
            header.availability_condition()
        );
        let active = exhausted
            .root_sources()
            .iter()
            .filter(|root| {
                selected(&planner.conditions, root.condition(), &choices)
                    && root.node() == instance_nodes[&values[&root.source()]]
            })
            .collect::<Vec<_>>();
        assert_eq!(active.len(), 1);
        let two_round_node = active[0].node();
        let (source, current) = replay_captured_edge_presence(
            &planner.conditions,
            graph,
            exit,
            exhaustion,
            exit.owner(),
            &values,
            &instance_nodes,
            &captured_slots,
            &mut choices,
        );
        assert_eq!(source, active[0].source());
        assert_eq!(values.remove(&source), Some(current));
        assert!(values.insert(exit.owner(), current).is_none());
        let root_drops = planner
            .facts
            .iter()
            .filter(|fact| fact.target() == DropTarget::Named(header.symbol()))
            .collect::<Vec<_>>();
        assert_eq!(root_drops.len(), 1);
        let root_drop = *root_drops[0];
        assert_eq!(root_drop.owner(), Some(exit.owner()));
        assert_eq!(root_drop.condition(), Some(exit.availability_condition()));
        let release = planner
            .cleanup
            .iter()
            .find(|(point, action)| {
                *point == root_drop.point()
                    && matches!(action, IterationCleanupAction::ReleaseClosureInstances {
                        layout: ClosureReleaseLayout::Iteration(release_statement),
                        root,
                    } if *release_statement == statement && *root == root_drop)
            })
            .unwrap()
            .1;
        let IterationCleanupAction::ReleaseClosureInstances {
            layout: ClosureReleaseLayout::Iteration(release_statement),
            root: release_root,
        } = release
        else {
            unreachable!()
        };
        assert_eq!(release_statement, statement);
        assert_eq!(release_root, root_drop);
        assert!(!planner.cleanup.iter().any(|(_, action)| {
            matches!(action, IterationCleanupAction::Drop(fact) if *fact == root_drop)
        }));
        let DropPoint::CallReturn(call) = root_drop.point() else {
            panic!("final f call must release the carried root")
        };
        assert_eq!(
            sources
                .slice(parsed.ast().expressions().get(call).unwrap().span())
                .unwrap(),
            "f()"
        );
        let Some(CleanupOwnerValue::Closure {
            expression: initial_closure,
            ..
        }) = planner.conditions.owner_value(entry.values()[0].source())
        else {
            panic!("entry must come from the initial closure formation")
        };
        let recursive_node = *header
            .root_nodes()
            .iter()
            .find(|&&node| graph.nodes()[node].closure() == recursive_closure)
            .unwrap();
        let initial_node = *header
            .root_nodes()
            .iter()
            .find(|&&node| graph.nodes()[node].closure() == *initial_closure)
            .unwrap();
        assert_ne!(recursive_node, initial_node);
        let captured_edge = graph.nodes()[recursive_node]
            .sources()
            .iter()
            .find(|source| source.capture().source() == input.source())
            .unwrap();
        assert_eq!(captured_edge.position(), capture_slot.position());
        assert_eq!(instance_nodes[&seed], initial_node);
        assert_eq!(instance_nodes[&rounds[0]], recursive_node);
        assert_eq!(instance_nodes[&rounds[1]], recursive_node);
        assert_eq!(entry_root.node(), initial_node);
        assert_eq!(backedge.root_sources()[0].node(), recursive_node);
        assert_eq!(two_round_node, recursive_node);
        // 零轮沿 Exhaustion 根转发，再用已记录的根 drop 释放入口实例。
        let mut zero_round = zero_round;
        let active = exhausted
            .root_sources()
            .iter()
            .filter(|root| {
                selected(&planner.conditions, root.condition(), &zero_choices)
                    && root.node() == instance_nodes[&zero_round[&root.source()]]
            })
            .collect::<Vec<_>>();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].node(), initial_node);
        let (source, zero_root) = replay_captured_edge_presence(
            &planner.conditions,
            graph,
            exit,
            exhaustion,
            exit.owner(),
            &zero_round,
            &instance_nodes,
            &BTreeMap::new(),
            &mut zero_choices,
        );
        assert_eq!(source, active[0].source());
        assert_eq!(zero_round.remove(&source), Some(zero_root));
        assert_eq!(zero_root, seed);
        assert!(zero_round.insert(exhausted.target(), zero_root).is_none());
        let zero_root = zero_round.remove(&release_root.owner().unwrap()).unwrap();
        assert_eq!(
            replay_owned_closure_release(graph, &instance_nodes, &mut BTreeMap::new(), zero_root),
            [seed]
        );
        assert!(zero_round.is_empty());
        // 动作只给出根与图身份；逐实例释放仍由测试从已形成实例的槽模拟。
        let root_instance = values.remove(&release_root.owner().unwrap()).unwrap();
        let released = replay_owned_closure_release(
            &planner.loop_capture_graphs[&release_statement.index()],
            &instance_nodes,
            &mut captured_slots,
            root_instance,
        );
        assert_eq!(released, [seed, rounds[0], rounds[1]]);
        assert!(captured_slots.is_empty());
    }

    #[test]
    fn conditional_seed_recursive_rebinding_jump_matrix() {
        for tail in ["", "continue", "if (stop) { break }"] {
            assert_conditional_seed_rebinding(tail);
        }
    }

    fn assert_conditional_seed_rebinding(tail: &str) {
        let breaking = tail == "if (stop) { break }";
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "conditional-recursive-seed.ko",
                format!("fun run(flag: Boolean, stop: Boolean, flags: List<Int>) {{\nvar f: move () -> Unit = if (flag) (move {{}}) else (move {{}})\nfor (_ in flags) {{ f = move {{ f() }}\n{tail} }}\nval used = f() }}"),
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some(), "{tail}");
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let crate::parser::Statement::For { source, body, .. } =
            parsed.ast().statements().get(statement).unwrap().payload()
        else {
            panic!("expected for loop");
        };
        let candidate = planner.into_candidate_facts();
        let [plan] = candidate.iterations.as_slice() else {
            panic!("one loop plan must be assembled for {tail}")
        };
        assert_eq!(plan.descriptor().statement(), statement);
        let table = &candidate.cleanup_conditions;
        let steps = &candidate.cleanup_steps;
        let graph = plan.capture_graph();
        let phis = plan.closure_phis();
        let header = phis
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
            .unwrap();
        let exit = phis
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
            .unwrap();
        let incoming = |kind| {
            plan.closure_phi_incomings()
                .iter()
                .find(|edge| edge.kind() == kind)
                .unwrap()
        };
        let entry = incoming(IterationPhiIncomingKind::Entry);
        let jump = plan
            .closure_phi_incomings()
            .iter()
            .find(|edge| match (tail, edge.kind()) {
                ("", IterationPhiIncomingKind::Fallthrough)
                | ("continue", IterationPhiIncomingKind::Continue(_))
                | ("if (stop) { break }", IterationPhiIncomingKind::Break(_)) => true,
                _ => false,
            })
            .unwrap();
        let exhaustion = incoming(IterationPhiIncomingKind::Exhaustion);
        assert_eq!(entry.point(), DropPoint::AfterExpression(*source));
        assert_eq!(entry.boundary(), IterationPhiBoundary::Header);
        let jump_boundary = if breaking {
            IterationPhiBoundary::Exit
        } else {
            IterationPhiBoundary::Header
        };
        assert_eq!(jump.boundary(), jump_boundary);
        match jump.point() {
            DropPoint::AfterStatement(completed) if tail.is_empty() => {
                assert_eq!(completed, *body);
            }
            DropPoint::ControlTransfer(control) if !tail.is_empty() => {
                assert_eq!(
                    sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
                    Ok(if breaking { "break" } else { tail })
                );
            }
            other => panic!("{tail} has the wrong phi execution point: {other:?}"),
        }
        assert_eq!(exhaustion.point(), DropPoint::LoopExit(statement));
        assert_eq!(exhaustion.boundary(), IterationPhiBoundary::Exit);
        fn binding(
            edge: &crate::ownership_checking::IterationPhiIncoming,
            owner: CleanupOwnerValueId,
        ) -> &IterationPhiIncomingBinding {
            edge.bindings()
                .iter()
                .find(|binding| binding.target() == owner)
                .unwrap()
        }
        fn active_root(
            table: &CleanupConditions,
            binding: &IterationPhiIncomingBinding,
            owners: &BTreeMap<CleanupOwnerValueId, usize>,
            nodes: &BTreeMap<usize, usize>,
            choices: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
        ) -> Option<(usize, CleanupOwnerValueId)> {
            let available = selected(table, binding.available_when(), choices);
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), choices))
                .collect::<Vec<_>>();
            assert_eq!(values.len(), usize::from(available));
            let roots = binding
                .root_sources()
                .iter()
                .filter(|root| {
                    values
                        .first()
                        .is_some_and(|value| value.source() == root.source())
                        && selected(table, root.condition(), choices)
                        && nodes[&owners[&root.source()]] == root.node()
                })
                .collect::<Vec<_>>();
            assert_eq!(roots.len(), values.len());
            roots.first().map(|root| {
                assert_eq!(root.source(), values[0].source());
                (root.node(), root.source())
            })
        }
        let entry_binding = binding(entry, header.owner());
        let jump_target = if breaking { exit } else { header };
        let jump_binding = binding(jump, jump_target.owner());
        let exit_binding = binding(exhaustion, exit.owner());
        let [entry_value] = entry_binding.values() else {
            panic!("the seed must have one snapshot source")
        };
        let seed_snapshot = table.owner_snapshot(entry_value.source()).unwrap();
        assert_eq!(seed_snapshot.capture_inputs().len(), 2);
        let control = seed_snapshot.copies()[0].source();
        assert!(matches!(
            table.selector(control).unwrap().source(),
            CleanupSelectorSource::Control(_)
        ));
        fn unique_action_index(
            steps: &[(DropPoint, IterationCleanupAction)],
            matches: impl Fn(&IterationCleanupAction) -> bool,
        ) -> usize {
            let found = steps
                .iter()
                .enumerate()
                .filter_map(|(index, (_, action))| matches(action).then_some(index))
                .collect::<Vec<_>>();
            let [index] = found.as_slice() else {
                panic!("one matching formation action must exist")
            };
            *index
        }
        let capture_at = unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::SaveClosureCapture { input, .. }
                if input.value() == CleanupCaptureValue::Owner(header.owner()))
        });
        let recursive_create = steps[capture_at].1;
        let IterationCleanupAction::SaveClosureCapture {
            owner: recursive_owner,
            target: recursive_slot,
            input: recursive_input,
        } = recursive_create
        else {
            unreachable!()
        };
        assert_eq!(
            unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::SaveClosureCapture { owner, .. }
                    if *owner == recursive_owner)
            }),
            capture_at
        );
        let create_at = unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::CreateClosureOwner { owner, .. }
                if *owner == recursive_owner)
        });
        let IterationCleanupAction::CreateClosureOwner {
            closure: recursive_closure,
            ..
        } = steps[create_at].1
        else {
            unreachable!()
        };
        assert_eq!(
            unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::CreateClosureOwner { closure, .. }
                    if *closure == recursive_closure)
            }),
            create_at
        );
        let snapshot_at = unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::SaveOwnerSnapshot { value, .. }
                if *value == recursive_closure)
        });
        let IterationCleanupAction::SaveOwnerSnapshot {
            owner: recursive_snapshot,
            ..
        } = steps[snapshot_at].1
        else {
            unreachable!()
        };
        assert_eq!(
            unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner, .. }
                    if *owner == recursive_snapshot)
            }),
            snapshot_at
        );
        let commit_at = unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, .. }
                if *owner == recursive_snapshot)
        });
        let formation_point = DropPoint::AfterExpression(recursive_closure);
        assert!(create_at < capture_at && capture_at < snapshot_at && snapshot_at < commit_at);
        for index in [create_at, capture_at, snapshot_at, commit_at] {
            assert_eq!(steps[index].0, formation_point);
        }
        let formation = [create_at, capture_at, snapshot_at, commit_at]
            .iter()
            .map(|&index| steps[index].1)
            .collect::<Vec<_>>();
        let recursive_node = graph
            .nodes()
            .iter()
            .position(|node| node.closure() == recursive_closure)
            .unwrap();
        assert_eq!(jump_binding.values()[0].source(), recursive_snapshot);
        let capture_position = table.capture_slot_value(recursive_slot).unwrap().position();
        assert!(matches!(formation.as_slice(), [
            IterationCleanupAction::CreateClosureOwner { owner, closure },
            IterationCleanupAction::SaveClosureCapture { owner: captured, target, input },
            IterationCleanupAction::SaveOwnerSnapshot { owner: snapshot, value, .. },
            IterationCleanupAction::CommitOwnerSnapshot { owner: committed, target: symbol },
        ] if *owner == recursive_owner
            && *closure == recursive_closure
            && *captured == recursive_owner
            && *target == recursive_slot
            && *input == recursive_input
            && *snapshot == recursive_snapshot
            && *value == recursive_closure
            && *committed == recursive_snapshot
            && *symbol == header.symbol()));
        assert!(!steps.iter().any(|(point, action)| {
            *point == formation_point
                && matches!(action,
                    IterationCleanupAction::Drop(fact)
                    | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. }
                    if fact.target() == DropTarget::Named(header.symbol())
                        || fact.owner() == Some(header.owner()))
        }));
        let final_call = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()) == Ok("f()"))
            .max_by_key(|(_, node)| node.span().start())
            .map(|(id, _)| id)
            .unwrap();
        let releases = steps
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(loop_id),
                    root,
                } if *loop_id == statement
                    && root.owner() == Some(exit.owner())
                    && root.target() == DropTarget::Named(header.symbol()) =>
                {
                    Some((*point, *root))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(point, release)] = releases.as_slice() else {
            panic!("final f root must have one instance release")
        };
        assert_eq!(*point, DropPoint::CallReturn(final_call));
        for branch in 0..2 {
            for rounds in [0, if breaking { 1 } else { 2 }] {
                let mut choices = BTreeMap::from([(control, branch)]);
                let mut executed_points = Vec::new();
                let seed_input = seed_snapshot
                    .capture_inputs()
                    .iter()
                    .filter(|input| selected(&table, input.condition(), &choices))
                    .collect::<Vec<_>>();
                let [seed_input] = seed_input.as_slice() else {
                    panic!("one branch must form the seed environment")
                };
                let watched = [
                    seed_input.owner(),
                    entry_value.source(),
                    header.owner(),
                    recursive_owner,
                    recursive_snapshot,
                    exit.owner(),
                ];
                let root_actions_at = |at: DropPoint,
                                       choices: &BTreeMap<
                    crate::ownership_checking::CleanupSelectorId,
                    usize,
                >| {
                    steps
                        .iter()
                        .filter(|(point, _)| *point == at)
                        .filter_map(|(_, action)| {
                            let (fact, instance_release) = match action {
                                IterationCleanupAction::Drop(fact) => (fact, false),
                                IterationCleanupAction::ReleaseClosureInstances {
                                    root, ..
                                } => (root, true),
                                _ => return None,
                            };
                            let address_root = fact
                                .instance_address()
                                .map(|address| table.instance_address(address).unwrap().root());
                            ((fact.owner().is_some_and(|owner| watched.contains(&owner))
                                || address_root.is_some_and(|owner| watched.contains(&owner))
                                || fact.target() == DropTarget::Named(header.symbol()))
                                && fact
                                    .condition()
                                    .is_none_or(|guard| selected(&table, guard, choices)))
                            .then_some((instance_release, *fact))
                        })
                        .collect::<Vec<_>>()
                };
                let seed_closure = steps
                    .iter()
                    .find_map(|(_, action)| match action {
                        IterationCleanupAction::CreateClosureOwner { owner, closure }
                            if *owner == seed_input.owner() =>
                        {
                            Some(*closure)
                        }
                        _ => None,
                    })
                    .unwrap();
                let seed_node = graph
                    .nodes()
                    .iter()
                    .position(|node| node.closure() == seed_closure)
                    .unwrap();
                assert_ne!(seed_node, recursive_node);
                let seed_create = steps
                    .iter()
                    .find(|(_, action)| {
                        matches!(action,
                        IterationCleanupAction::CreateClosureOwner { owner, closure }
                        if *owner == seed_input.owner() && *closure == seed_closure)
                    })
                    .unwrap();
                assert_eq!(seed_create.0, DropPoint::AfterExpression(seed_closure));
                assert!(root_actions_at(seed_create.0, &choices).is_empty());
                executed_points.push((seed_create.0, choices.clone()));
                let seed_save = steps
                    .iter()
                    .find(|(_, action)| {
                        matches!(action,
                        IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                        if *owner == entry_value.source() && *value == seed_snapshot.value())
                    })
                    .unwrap();
                assert_eq!(
                    seed_save.0,
                    DropPoint::AfterExpression(seed_snapshot.value())
                );
                let seed_create_at = steps.iter().position(|step| step == seed_create).unwrap();
                let seed_save_at = steps.iter().position(|step| step == seed_save).unwrap();
                assert!(seed_create_at < seed_save_at);
                let mut nodes = BTreeMap::new();
                let mut owners = BTreeMap::new();
                let mut captured = BTreeMap::new();
                let IterationCleanupAction::CreateClosureOwner { owner, closure } = seed_create.1
                else {
                    unreachable!()
                };
                assert_eq!(closure, seed_closure);
                assert!(nodes.insert(1_usize, seed_node).is_none());
                assert!(owners.insert(owner, 1_usize).is_none());
                let IterationCleanupAction::SaveOwnerSnapshot {
                    condition,
                    owner,
                    value,
                } = seed_save.1
                else {
                    unreachable!()
                };
                assert_eq!(value, seed_snapshot.value());
                assert!(condition.is_none_or(|guard| { selected(&table, guard, &choices) }));
                let inputs = seed_snapshot
                    .capture_inputs()
                    .iter()
                    .filter(|input| selected(&table, input.condition(), &choices))
                    .collect::<Vec<_>>();
                assert_eq!(inputs.len(), 1);
                let formed = owners.remove(&inputs[0].owner()).unwrap();
                assert!(owners.insert(owner, formed).is_none());
                replay_snapshot_choices(&table, owner, &mut choices);
                assert!(root_actions_at(seed_save.0, &choices).is_empty());
                executed_points.push((seed_save.0, choices.clone()));
                let entry_root =
                    active_root(&table, entry_binding, &owners, &nodes, &choices).unwrap();
                assert_eq!(entry_root, (seed_node, entry_value.source()));
                assert!(root_actions_at(entry.point(), &choices).is_empty());
                let entry_values = replay_captured_edge(
                    &table,
                    graph,
                    phis,
                    entry,
                    &owners,
                    &nodes,
                    &captured,
                    &mut choices,
                );
                assert_eq!(entry_values.len(), 1);
                assert_eq!(entry_values[&header.owner()], (entry_value.source(), 1));
                executed_points.push((entry.point(), choices.clone()));
                assert_eq!(owners.remove(&entry_value.source()), Some(1));
                assert!(owners.insert(header.owner(), 1).is_none());
                let mut chain = vec![1_usize];
                for round in 0..rounds {
                    let instance = round + 2;
                    let before_formation = choices.clone();
                    for action in &formation {
                        match action {
                            IterationCleanupAction::CreateClosureOwner { owner, closure } => {
                                assert_eq!(
                                    (*owner, *closure),
                                    (recursive_owner, recursive_closure)
                                );
                                assert!(nodes.insert(instance, recursive_node).is_none());
                                assert!(owners.insert(*owner, instance).is_none());
                            }
                            IterationCleanupAction::SaveClosureCapture {
                                owner,
                                target,
                                input,
                            } => {
                                assert_eq!(*owner, recursive_owner);
                                assert!(selected(&table, input.condition(), &choices));
                                assert_eq!(input.effect(), ClosureCaptureEffect::Move);
                                let CleanupCaptureValue::Owner(source) = input.value() else {
                                    panic!("recursive capture must read the old header")
                                };
                                let old = owners.remove(&source).unwrap();
                                let position =
                                    table.capture_slot_value(*target).unwrap().position();
                                assert_eq!(position, capture_position);
                                assert!(captured.insert((owners[owner], position), old).is_none());
                            }
                            IterationCleanupAction::SaveOwnerSnapshot {
                                condition,
                                owner,
                                value,
                            } => {
                                assert_eq!(
                                    (*owner, *value),
                                    (recursive_snapshot, recursive_closure)
                                );
                                assert!(
                                    condition.is_none_or(|guard| selected(&table, guard, &choices))
                                );
                                let snapshot = table.owner_snapshot(*owner).unwrap();
                                let inputs = snapshot
                                    .capture_inputs()
                                    .iter()
                                    .filter(|input| selected(&table, input.condition(), &choices))
                                    .collect::<Vec<_>>();
                                assert_eq!(inputs.len(), 1);
                                let formed = owners.remove(&inputs[0].owner()).unwrap();
                                assert!(owners.insert(*owner, formed).is_none());
                                replay_snapshot_choices(&table, *owner, &mut choices);
                            }
                            IterationCleanupAction::CommitOwnerSnapshot { owner, target } => {
                                assert_eq!(
                                    (*owner, *target),
                                    (recursive_snapshot, header.symbol())
                                );
                                assert_eq!(owners[owner], instance);
                            }
                            _ => unreachable!(),
                        }
                    }
                    assert!(root_actions_at(formation_point, &before_formation).is_empty());
                    assert!(root_actions_at(formation_point, &choices).is_empty());
                    executed_points.push((formation_point, before_formation));
                    executed_points.push((formation_point, choices.clone()));
                    if breaking {
                        let Some(CleanupCondition::Choice { selector, branches }) =
                            table.get(jump.condition())
                        else {
                            panic!("break edge must depend on stop")
                        };
                        assert_eq!(
                            branches,
                            &[CleanupConditionId::ALWAYS, CleanupConditionId::NEVER]
                        );
                        let control = table.selector(*selector).unwrap().control().unwrap();
                        assert_eq!(
                            sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
                            Ok(tail)
                        );
                        let mut skipped = choices.clone();
                        skipped.insert(*selector, 1);
                        assert!(!selected(&table, jump.condition(), &skipped));
                        choices.insert(*selector, 0);
                    }
                    let jump_root =
                        active_root(&table, jump_binding, &owners, &nodes, &choices).unwrap();
                    assert_eq!(jump_root, (recursive_node, recursive_snapshot));
                    assert!(root_actions_at(jump.point(), &choices).is_empty());
                    let jump_values = replay_captured_edge(
                        &table,
                        graph,
                        phis,
                        jump,
                        &owners,
                        &nodes,
                        &captured,
                        &mut choices,
                    );
                    assert_eq!(jump_values.len(), 1);
                    assert_eq!(
                        jump_values[&jump_target.owner()],
                        (recursive_snapshot, instance)
                    );
                    executed_points.push((jump.point(), choices.clone()));
                    assert_eq!(owners.remove(&recursive_snapshot), Some(instance));
                    assert!(owners.insert(jump_target.owner(), instance).is_none());
                    chain.push(instance);
                }
                if breaking && rounds > 0 {
                    assert!(!owners.contains_key(&header.owner()));
                    assert_eq!(owners[&exit.owner()], chain[rounds]);
                } else {
                    let exit_root =
                        active_root(&table, exit_binding, &owners, &nodes, &choices).unwrap();
                    assert_eq!(exit_root.0, *nodes.get(&chain[rounds]).unwrap());
                    assert_eq!(exit_root.1, header.owner());
                    assert!(root_actions_at(exhaustion.point(), &choices).is_empty());
                    let exit_values = replay_captured_edge(
                        &table,
                        graph,
                        phis,
                        exhaustion,
                        &owners,
                        &nodes,
                        &captured,
                        &mut choices,
                    );
                    assert_eq!(exit_values.len(), 1);
                    assert_eq!(exit_values[&exit.owner()], (header.owner(), chain[rounds]));
                    executed_points.push((exhaustion.point(), choices.clone()));
                    let root = owners.remove(&header.owner()).unwrap();
                    assert!(owners.insert(exit.owner(), root).is_none());
                }
                executed_points.push((DropPoint::AfterStatement(statement), choices.clone()));
                assert!(
                    release
                        .condition()
                        .is_none_or(|guard| { selected(&table, guard, &choices) })
                );
                assert_eq!(root_actions_at(*point, &choices), [(true, *release)]);
                executed_points.push((*point, choices.clone()));
                executed_points.push((DropPoint::AfterExpression(final_call), choices.clone()));
                let selected_root_actions = executed_points
                    .iter()
                    .flat_map(|(at, at_choices)| {
                        root_actions_at(*at, at_choices)
                            .into_iter()
                            .map(|(instance_release, fact)| (*at, instance_release, fact))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    selected_root_actions,
                    [(*point, true, *release)],
                    "{tail} branch {branch}, rounds {rounds} must release only at f()"
                );
                let root = owners.remove(&release.owner().unwrap()).unwrap();
                assert_eq!(root, chain[rounds]);
                let released = replay_owned_closure_release(graph, &nodes, &mut captured, root);
                assert_eq!(released, chain);
                assert!(captured.is_empty());
                assert!(owners.is_empty());
            }
        }
    }

    #[test]
    fn alternating_recursive_capture_releases_the_formed_instance_chain() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "alternating-recursive-capture.ko",
                "fun run(flags: List<Int>) { var f: move () -> Unit = move {}\nvar g: move () -> Unit = move {}\nfor (_ in flags) { { f = move { g() } }\ng = move { f() } }\nval used = g() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some());
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let graph = &planner.loop_capture_graphs[&statement.index()];
        let f_node = graph
            .nodes()
            .iter()
            .position(|node| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                ) == Ok("move { g() }")
            })
            .unwrap();
        let g_node = graph
            .nodes()
            .iter()
            .position(|node| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                ) == Ok("move { f() }")
            })
            .unwrap();
        assert!(
            graph.nodes()[f_node].sources()[0]
                .captured()
                .contains(&g_node)
        );
        assert!(
            graph.nodes()[g_node].sources()[0]
                .captured()
                .contains(&f_node)
        );
        for node in [f_node, g_node] {
            assert_eq!(
                graph.nodes()[node].sources()[0].capture().effect(),
                ClosureCaptureEffect::Move
            );
        }
        let ClosureCaptureSource::Symbol(f_symbol) =
            graph.nodes()[g_node].sources()[0].capture().source()
        else {
            panic!("g must capture f")
        };
        let ClosureCaptureSource::Symbol(g_symbol) =
            graph.nodes()[f_node].sources()[0].capture().source()
        else {
            panic!("f must capture g")
        };
        let phis = &planner.loop_phis[&statement.index()];
        let phi = |symbol, boundary| {
            phis.iter()
                .find(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
                .unwrap()
        };
        let f_header = phi(f_symbol, IterationPhiBoundary::Header);
        let g_header = phi(g_symbol, IterationPhiBoundary::Header);
        let g_exit = phi(g_symbol, IterationPhiBoundary::Exit);
        let incoming = |kind| {
            planner.loop_phi_incomings[&statement.index()]
                .iter()
                .find(|edge| edge.kind() == kind)
                .unwrap()
        };
        let binding = |kind, target| {
            incoming(kind)
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap()
        };
        let entry_g = binding(IterationPhiIncomingKind::Entry, g_header.owner());
        let back_g = binding(IterationPhiIncomingKind::Fallthrough, g_header.owner());
        let exhausted_g = binding(IterationPhiIncomingKind::Exhaustion, g_exit.owner());
        assert_eq!(entry_g.values().len(), 1);
        assert_eq!(back_g.values().len(), 1);
        assert_eq!(exhausted_g.values().len(), 1);
        assert_eq!(exhausted_g.values()[0].source(), g_header.owner());
        for kind in [
            IterationPhiIncomingKind::Entry,
            IterationPhiIncomingKind::Fallthrough,
        ] {
            let f = binding(kind, f_header.owner());
            assert!(f.values().is_empty(), "f is moved into g in every round");
            assert_eq!(f.available_when(), CleanupConditionId::NEVER);
            assert!(f.capture_slots_to_clear().is_empty());
        }
        let steps = &planner.cleanup;
        let created = |closure| {
            steps
                .iter()
                .enumerate()
                .find_map(|(index, (_, action))| match action {
                    IterationCleanupAction::CreateClosureOwner {
                        owner,
                        closure: formed,
                    } if *formed == closure => Some((index, *owner)),
                    _ => None,
                })
                .unwrap()
        };
        let (f_create_at, f_created) = created(graph.nodes()[f_node].closure());
        let (g_create_at, g_created) = created(graph.nodes()[g_node].closure());
        let saved = |owner| {
            steps
                .iter()
                .enumerate()
                .find_map(|(index, (_, action))| match action {
                    IterationCleanupAction::SaveClosureCapture {
                        owner: saved,
                        target,
                        input,
                    } if *saved == owner => Some((index, *target, *input)),
                    _ => None,
                })
                .unwrap()
        };
        let (f_save_at, f_slot, f_input) = saved(f_created);
        let (g_save_at, g_slot, g_input) = saved(g_created);
        let snapshot = |closure| {
            steps
                .iter()
                .enumerate()
                .find_map(|(index, (_, action))| match action {
                    IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                        if *value == closure =>
                    {
                        Some((index, *owner))
                    }
                    _ => None,
                })
                .unwrap()
        };
        let (f_snapshot_at, f_snapshot) = snapshot(graph.nodes()[f_node].closure());
        let (g_snapshot_at, g_snapshot) = snapshot(graph.nodes()[g_node].closure());
        let committed = |owner, symbol| {
            steps
                .iter()
                .position(|(_, action)| {
                    matches!(action, IterationCleanupAction::CommitOwnerSnapshot {
                        owner: saved,
                        target,
                    } if *saved == owner && *target == symbol)
                })
                .unwrap()
        };
        let f_commit_at = committed(f_snapshot, f_symbol);
        let g_commit_at = committed(g_snapshot, g_symbol);
        assert_eq!(
            f_input.value(),
            CleanupCaptureValue::Owner(g_header.owner())
        );
        assert_eq!(g_input.value(), CleanupCaptureValue::Owner(f_snapshot));
        assert_eq!(back_g.values()[0].source(), g_snapshot);
        assert_eq!(
            (f_input.mode(), f_input.effect()),
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
        );
        assert_eq!(
            (g_input.mode(), g_input.effect()),
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
        );
        let f_layout = planner.conditions.capture_slot_value(f_slot).unwrap();
        let g_layout = planner.conditions.capture_slot_value(g_slot).unwrap();
        assert_eq!(
            (
                f_layout.environment(),
                f_layout.closure(),
                f_layout.source()
            ),
            (f_created, graph.nodes()[f_node].closure(), f_input.source())
        );
        assert_eq!(
            (
                g_layout.environment(),
                g_layout.closure(),
                g_layout.source()
            ),
            (g_created, graph.nodes()[g_node].closure(), g_input.source())
        );
        let f_position = f_layout.position();
        let g_position = g_layout.position();
        assert_eq!((f_position, g_position), (0, 0));
        assert!(
            f_create_at < f_save_at && f_save_at < f_snapshot_at && f_snapshot_at < f_commit_at
        );
        assert!(
            f_commit_at < g_create_at
                && g_create_at < g_save_at
                && g_save_at < g_snapshot_at
                && g_snapshot_at < g_commit_at
        );
        for (owner, formed) in [(f_snapshot, f_created), (g_snapshot, g_created)] {
            let inputs = planner
                .conditions
                .owner_snapshot(owner)
                .unwrap()
                .capture_inputs();
            assert_eq!(inputs.len(), 1);
            assert_eq!(inputs[0].owner(), formed);
        }
        let initial_owner = entry_g.values()[0].source();
        let initial_create = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *owner == initial_owner =>
                {
                    Some(*closure)
                }
                _ => None,
            })
            .unwrap();
        let mut next_instance = 0;
        let mut owners = BTreeMap::new();
        let mut instance_nodes = BTreeMap::new();
        let mut create =
            |owner, closure, owners: &mut BTreeMap<_, _>, nodes: &mut BTreeMap<_, _>| {
                next_instance += 1;
                let node = graph
                    .nodes()
                    .iter()
                    .position(|node| node.closure() == closure)
                    .unwrap();
                assert!(owners.insert(owner, next_instance).is_none());
                assert!(nodes.insert(next_instance, node).is_none());
                next_instance
            };
        let initial_g = create(
            initial_owner,
            initial_create,
            &mut owners,
            &mut instance_nodes,
        );
        let mut choices = BTreeMap::new();
        let mut captured = BTreeMap::new();
        let entry_values = replay_captured_edge(
            &planner.conditions,
            graph,
            phis,
            incoming(IterationPhiIncomingKind::Entry),
            &owners,
            &instance_nodes,
            &captured,
            &mut choices,
        );
        assert_eq!(entry_values.len(), 1);
        assert_eq!(entry_values[&g_header.owner()], (initial_owner, initial_g));
        assert_eq!(choices[&f_header.availability_selector()], 0);
        assert_eq!(choices[&g_header.availability_selector()], 1);
        assert_eq!(owners.remove(&initial_owner), Some(initial_g));
        assert!(owners.insert(g_header.owner(), initial_g).is_none());
        let zero_round = owners.clone();
        let zero_choices = choices.clone();
        let mut rounds = Vec::new();
        let mut one_round = None;
        let f_drops = steps
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::Drop(fact)
                    if fact.owner() == Some(f_header.owner())
                        && fact.target() == DropTarget::Named(f_symbol) =>
                {
                    Some(*fact)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(f_drops.len(), 2);
        let f_after_expression = f_drops
            .iter()
            .find(|fact| matches!(fact.point(), DropPoint::AfterExpression(_)))
            .unwrap();
        let f_at_exit = f_drops
            .iter()
            .find(|fact| fact.point() == DropPoint::LoopExit(statement))
            .unwrap();
        for _ in 0..2 {
            assert!(selected(&planner.conditions, f_input.condition(), &choices));
            let f = create(
                f_created,
                graph.nodes()[f_node].closure(),
                &mut owners,
                &mut instance_nodes,
            );
            let prior_g = owners.remove(&g_header.owner()).unwrap();
            assert!(captured.insert((f, f_position), prior_g).is_none());
            assert_eq!(owners.remove(&f_created), Some(f));
            assert!(owners.insert(f_snapshot, f).is_none());
            replay_snapshot_choices(&planner.conditions, f_snapshot, &mut choices);
            assert!(!selected(
                &planner.conditions,
                f_after_expression
                    .condition()
                    .unwrap_or(CleanupConditionId::ALWAYS),
                &choices
            ));
            let g = create(
                g_created,
                graph.nodes()[g_node].closure(),
                &mut owners,
                &mut instance_nodes,
            );
            assert!(selected(&planner.conditions, g_input.condition(), &choices));
            assert_eq!(owners.remove(&f_snapshot), Some(f));
            assert!(captured.insert((g, g_position), f).is_none());
            assert_eq!(owners.remove(&g_created), Some(g));
            assert!(owners.insert(g_snapshot, g).is_none());
            replay_snapshot_choices(&planner.conditions, g_snapshot, &mut choices);
            let back_values = replay_captured_edge(
                &planner.conditions,
                graph,
                phis,
                incoming(IterationPhiIncomingKind::Fallthrough),
                &owners,
                &instance_nodes,
                &captured,
                &mut choices,
            );
            assert_eq!(back_values.len(), 1);
            assert_eq!(back_values[&g_header.owner()], (g_snapshot, g));
            assert_eq!(choices[&f_header.availability_selector()], 0);
            assert_eq!(choices[&g_header.availability_selector()], 1);
            assert_eq!(owners.remove(&back_g.values()[0].source()), Some(g));
            assert!(owners.insert(back_g.target(), g).is_none());
            rounds.extend([f, g]);
            if one_round.is_none() {
                one_round = Some((
                    owners.clone(),
                    captured.clone(),
                    instance_nodes.clone(),
                    choices.clone(),
                ));
            }
        }
        let releases = steps
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(loop_id),
                    root,
                } if *loop_id == statement && root.owner() == Some(g_exit.owner()) => Some(*root),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [release] = releases.as_slice() else {
            panic!("one exit root release is required: {releases:?}")
        };
        assert_eq!(release.target(), DropTarget::Named(g_symbol));
        let DropPoint::CallReturn(call) = release.point() else {
            panic!("the final g call must release its environment")
        };
        assert_eq!(
            sources.slice(parsed.ast().expressions().get(call).unwrap().span()),
            Ok("g()")
        );
        let exit_root = |mut owners: BTreeMap<CleanupOwnerValueId, usize>,
                         mut captured: BTreeMap<(usize, usize), usize>,
                         nodes: &BTreeMap<usize, usize>,
                         mut choices: BTreeMap<_, _>| {
            assert!(!selected(
                &planner.conditions,
                f_at_exit.condition().unwrap_or(CleanupConditionId::ALWAYS),
                &choices
            ));
            let exit_values = replay_captured_edge(
                &planner.conditions,
                graph,
                phis,
                incoming(IterationPhiIncomingKind::Exhaustion),
                &owners,
                nodes,
                &captured,
                &mut choices,
            );
            assert_eq!(exit_values.len(), 1);
            let (source, instance) = exit_values[&exhausted_g.target()];
            assert_eq!(source, exhausted_g.values()[0].source());
            assert_eq!(owners.remove(&source), Some(instance));
            assert!(owners.insert(exhausted_g.target(), instance).is_none());
            let active_root_actions = steps
                .iter()
                .filter_map(|(point, action)| {
                    let fact = match action {
                        IterationCleanupAction::Drop(fact)
                        | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => {
                            fact
                        }
                        _ => return None,
                    };
                    let from_exit = fact.owner() == Some(g_exit.owner())
                        || fact.instance_address().is_some_and(|address| {
                            planner.conditions.instance_address(address).unwrap().root()
                                == g_exit.owner()
                        })
                        || (*point == release.point()
                            && fact.target() == DropTarget::Named(g_symbol));
                    (from_exit
                        && fact
                            .condition()
                            .is_none_or(|guard| selected(&planner.conditions, guard, &choices)))
                    .then_some((*point, *action))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                active_root_actions,
                [(
                    release.point(),
                    IterationCleanupAction::ReleaseClosureInstances {
                        layout: ClosureReleaseLayout::Iteration(statement),
                        root: *release,
                    },
                )]
            );
            let instance = owners.remove(&release.owner().unwrap()).unwrap();
            let released = replay_owned_closure_release(graph, nodes, &mut captured, instance);
            assert!(captured.is_empty());
            assert!(owners.is_empty());
            released
        };
        assert_eq!(
            exit_root(zero_round, BTreeMap::new(), &instance_nodes, zero_choices),
            [initial_g]
        );
        let (one_owners, one_captured, one_nodes, one_choices) = one_round.unwrap();
        assert_eq!(
            exit_root(one_owners, one_captured, &one_nodes, one_choices),
            [initial_g, rounds[0], rounds[1]]
        );
        assert_eq!(
            exit_root(owners, captured, &instance_nodes, choices),
            [initial_g, rounds[0], rounds[1], rounds[2], rounds[3]]
        );
    }

    #[test]
    fn alternating_recursive_capture_jump_edges_keep_the_formed_chain() {
        for (jump, tail, boundary, rounds) in [
            (
                "continue",
                "continue",
                IterationPhiBoundary::Header,
                2_usize,
            ),
            (
                "break",
                "if (flag) { break }",
                IterationPhiBoundary::Exit,
                1_usize,
            ),
        ] {
            let mut sources = SourceMap::new();
            let source = sources
                .add_source(
                    "alternating-recursive-jump.ko",
                    format!("fun run(flags: List<Boolean>) {{ var f: move () -> Unit = move {{}}\nvar g: move () -> Unit = move {{}}\nfor (flag in flags) {{ {{ f = move {{ g() }} }}\ng = move {{ f() }}\n{tail} }}\nval used = g() }}"),
                )
                .unwrap();
            let parsed =
                crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                    .unwrap();
            assert!(parsed.diagnostics().is_empty(), "{jump}");
            let (names, types) = crate::type_checking::standard_environments();
            let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
            let typed =
                crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty(), "{jump}");
            let mut checker =
                super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
            let capture_liveness = super::super::capture_liveness(&checker).unwrap();
            checker.expression_live_after = capture_liveness.expression_after;
            checker.statement_live_after = capture_liveness.statement_after;
            let mut state = super::super::super::State::default();
            for &root in parsed.roots() {
                checker.check_item(root, &mut state).unwrap();
            }
            assert!(checker.diagnostics.is_empty(), "{jump}");
            let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
            let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
            let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
            for &root in parsed.roots() {
                planner.item(root).unwrap();
            }
            assert!(planner.recursive_capture_phi.is_some(), "{jump}");
            let statement = checker
                .iterations
                .values()
                .next()
                .unwrap()
                .descriptor()
                .statement();
            let candidate = planner.into_candidate_facts();
            let [plan] = candidate.iterations.as_slice() else {
                panic!("one loop plan must be assembled for {jump}")
            };
            assert_eq!(plan.descriptor().statement(), statement);
            let table = &candidate.cleanup_conditions;
            let graph = plan.capture_graph();
            let node = |literal| {
                graph
                    .nodes()
                    .iter()
                    .position(|node| {
                        sources.slice(
                            parsed
                                .ast()
                                .expressions()
                                .get(node.closure())
                                .unwrap()
                                .span(),
                        ) == Ok(literal)
                    })
                    .unwrap()
            };
            let f_node = node("move { g() }");
            let g_node = node("move { f() }");
            let ClosureCaptureSource::Symbol(f_symbol) =
                graph.nodes()[g_node].sources()[0].capture().source()
            else {
                panic!("g must capture f")
            };
            let ClosureCaptureSource::Symbol(g_symbol) =
                graph.nodes()[f_node].sources()[0].capture().source()
            else {
                panic!("f must capture g")
            };
            let phis = plan.closure_phis();
            let phi = |symbol, boundary| {
                phis.iter()
                    .find(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
                    .unwrap()
            };
            let f_header = phi(f_symbol, IterationPhiBoundary::Header);
            let g_header = phi(g_symbol, IterationPhiBoundary::Header);
            let g_exit = phi(g_symbol, IterationPhiBoundary::Exit);
            let incoming = |kind| {
                plan.closure_phi_incomings()
                    .iter()
                    .find(|edge| edge.kind() == kind)
                    .unwrap()
            };
            let entry = incoming(IterationPhiIncomingKind::Entry);
            let jump_edge = plan
                .closure_phi_incomings()
                .iter()
                .find(|edge| match (jump, edge.kind()) {
                    ("continue", IterationPhiIncomingKind::Continue(_))
                    | ("break", IterationPhiIncomingKind::Break(_)) => true,
                    _ => false,
                })
                .unwrap();
            assert_eq!(jump_edge.boundary(), boundary, "{jump}");
            let jump_target = phi(g_symbol, boundary);
            fn binding(
                edge: &crate::ownership_checking::IterationPhiIncoming,
                owner: CleanupOwnerValueId,
            ) -> &IterationPhiIncomingBinding {
                edge.bindings()
                    .iter()
                    .find(|binding| binding.target() == owner)
                    .unwrap()
            }
            let entry_g = binding(entry, g_header.owner());
            let jump_g = binding(jump_edge, jump_target.owner());
            assert_eq!(jump_g.values().len(), 1, "{jump}");
            for edge in [entry, jump_edge] {
                for binding in edge.bindings() {
                    assert!(binding.capture_slots_to_clear().is_empty(), "{jump}");
                    if phis.iter().any(|phi| {
                        phi.owner() == binding.target()
                            && phi
                                .root_nodes()
                                .iter()
                                .any(|&node| node == f_node || node == g_node)
                    }) {
                        assert!(binding.origins().is_empty(), "{jump}");
                    }
                }
            }
            assert!(binding(entry, f_header.owner()).values().is_empty());
            assert!(binding(entry, f_header.owner()).root_sources().is_empty());
            assert!(
                binding(jump_edge, phi(f_symbol, boundary).owner())
                    .values()
                    .is_empty()
            );
            assert!(
                binding(jump_edge, phi(f_symbol, boundary).owner())
                    .root_sources()
                    .is_empty()
            );
            let steps = &candidate.cleanup_steps;
            fn unique_action_index(
                steps: &[(DropPoint, IterationCleanupAction)],
                matches: impl Fn(&IterationCleanupAction) -> bool,
            ) -> usize {
                let found = steps
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (_, action))| matches(action).then_some(index))
                    .collect::<Vec<_>>();
                let [index] = found.as_slice() else {
                    panic!("one matching formation action must exist")
                };
                *index
            }
            let formed = |closure| {
                let index = unique_action_index(steps, |action| {
                    matches!(action, IterationCleanupAction::CreateClosureOwner { closure: formed, .. }
                        if *formed == closure)
                });
                let IterationCleanupAction::CreateClosureOwner { owner, .. } = steps[index].1
                else {
                    unreachable!()
                };
                owner
            };
            let f_owner = formed(graph.nodes()[f_node].closure());
            let g_owner = formed(graph.nodes()[g_node].closure());
            let capture = |owner| {
                let index = unique_action_index(steps, |action| {
                    matches!(action, IterationCleanupAction::SaveClosureCapture { owner: saved, .. }
                        if *saved == owner)
                });
                let IterationCleanupAction::SaveClosureCapture { target, input, .. } =
                    steps[index].1
                else {
                    unreachable!()
                };
                (target, input)
            };
            let (f_slot, f_input) = capture(f_owner);
            let (g_slot, g_input) = capture(g_owner);
            let snapshot = |closure| {
                let index = unique_action_index(steps, |action| {
                    matches!(action, IterationCleanupAction::SaveOwnerSnapshot { value, .. }
                        if *value == closure)
                });
                let IterationCleanupAction::SaveOwnerSnapshot { owner, .. } = steps[index].1 else {
                    unreachable!()
                };
                owner
            };
            let f_snapshot = snapshot(graph.nodes()[f_node].closure());
            let g_snapshot = snapshot(graph.nodes()[g_node].closure());
            let formation = |closure, owner, slot, input, saved, symbol| {
                let create_at = unique_action_index(steps, |action| {
                    matches!(action, IterationCleanupAction::CreateClosureOwner { owner: formed, .. }
                        if *formed == owner)
                });
                assert_eq!(
                    steps[create_at].1,
                    IterationCleanupAction::CreateClosureOwner { owner, closure }
                );
                let capture_at = unique_action_index(steps, |action| {
                    matches!(action, IterationCleanupAction::SaveClosureCapture { owner: saved_owner, .. }
                        if *saved_owner == owner)
                });
                assert_eq!(
                    steps[capture_at].1,
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target: slot,
                        input,
                    }
                );
                let snapshot_at = unique_action_index(steps, |action| {
                    matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner: snapshot, .. }
                        if *snapshot == saved)
                });
                let commit_at = unique_action_index(steps, |action| {
                    matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner: committed, .. }
                        if *committed == saved)
                });
                assert_eq!(
                    steps[commit_at].1,
                    IterationCleanupAction::CommitOwnerSnapshot {
                        owner: saved,
                        target: symbol,
                    }
                );
                assert!(
                    create_at < capture_at && capture_at < snapshot_at && snapshot_at < commit_at
                );
                for index in [create_at, capture_at, snapshot_at, commit_at] {
                    assert_eq!(steps[index].0, DropPoint::AfterExpression(closure));
                }
                let IterationCleanupAction::SaveOwnerSnapshot { condition, .. } =
                    steps[snapshot_at].1
                else {
                    unreachable!()
                };
                assert!(condition.is_none_or(|guard| guard == CleanupConditionId::ALWAYS));
                let snapshot = table.owner_snapshot(saved).unwrap();
                assert_eq!(snapshot.capture_inputs().len(), 1);
                assert_eq!(snapshot.capture_inputs()[0].owner(), owner);
                (create_at, commit_at)
            };
            let (_, f_commit_at) = formation(
                graph.nodes()[f_node].closure(),
                f_owner,
                f_slot,
                f_input,
                f_snapshot,
                f_symbol,
            );
            let (g_create_at, _) = formation(
                graph.nodes()[g_node].closure(),
                g_owner,
                g_slot,
                g_input,
                g_snapshot,
                g_symbol,
            );
            assert!(f_commit_at < g_create_at);
            assert_eq!(
                f_input.value(),
                CleanupCaptureValue::Owner(g_header.owner())
            );
            assert_eq!(g_input.value(), CleanupCaptureValue::Owner(f_snapshot));
            assert_eq!(jump_g.values()[0].source(), g_snapshot, "{jump}");
            let f_position = table.capture_slot_value(f_slot).unwrap().position();
            let g_position = table.capture_slot_value(g_slot).unwrap().position();
            let initial_owner = entry_g.values()[0].source();
            let initial_closure = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CreateClosureOwner { owner, closure }
                        if *owner == initial_owner =>
                    {
                        Some(*closure)
                    }
                    _ => None,
                })
                .unwrap();
            let initial_node = graph
                .nodes()
                .iter()
                .position(|node| node.closure() == initial_closure)
                .unwrap();
            let active_root = |binding: &IterationPhiIncomingBinding,
                               owners: &BTreeMap<_, _>,
                               nodes: &BTreeMap<_, _>,
                               choices: &BTreeMap<_, _>| {
                let roots = binding
                    .root_sources()
                    .iter()
                    .filter(|source| {
                        selected(&table, source.condition(), choices)
                            && nodes[&owners[&source.source()]] == source.node()
                    })
                    .collect::<Vec<_>>();
                let [root] = roots.as_slice() else {
                    panic!("{jump} must transport exactly one formed root")
                };
                **root
            };
            let active_root_actions = |owner, choices: &BTreeMap<_, _>| {
                steps
                    .iter()
                    .filter_map(|(point, action)| {
                        let (root, release) = match action {
                            IterationCleanupAction::Drop(root) => (root, false),
                            IterationCleanupAction::ReleaseClosureInstances { root, .. } => {
                                (root, true)
                            }
                            _ => return None,
                        };
                        ((root.owner() == Some(owner)
                            || root.instance_address().is_some_and(|address| {
                                table.instance_address(address).unwrap().root() == owner
                            }))
                            && root
                                .condition()
                                .is_none_or(|condition| selected(&table, condition, choices)))
                        .then_some((*point, release))
                    })
                    .collect::<Vec<_>>()
            };
            let mut instance_nodes = BTreeMap::from([(1_usize, initial_node)]);
            let mut owners = BTreeMap::from([(initial_owner, 1_usize)]);
            let mut captured = BTreeMap::new();
            let mut choices = BTreeMap::new();
            let entry_root = active_root(entry_g, &owners, &instance_nodes, &choices);
            assert_eq!(entry_root.source(), initial_owner);
            assert_eq!(entry_root.node(), initial_node);
            let entry_values = replay_captured_edge(
                &table,
                graph,
                phis,
                entry,
                &owners,
                &instance_nodes,
                &captured,
                &mut choices,
            );
            assert_eq!(entry_values.len(), 1);
            assert_eq!(entry_values[&g_header.owner()], (initial_owner, 1));
            assert_eq!(choices[&f_header.availability_selector()], 0, "{jump}");
            for root in g_header.root_origins() {
                assert_eq!(
                    choices[&root.selector()],
                    usize::from(root.node() == initial_node),
                    "{jump}"
                );
            }
            assert_eq!(owners.remove(&entry_root.source()), Some(1));
            assert!(owners.insert(g_header.owner(), 1).is_none());
            let mut next_instance = 1;
            for round in 0..rounds {
                assert!(selected(&table, f_input.condition(), &choices));
                next_instance += 1;
                let f_instance = next_instance;
                assert!(instance_nodes.insert(f_instance, f_node).is_none());
                assert!(owners.insert(f_owner, f_instance).is_none());
                let prior_g = owners.remove(&g_header.owner()).unwrap();
                assert!(captured.insert((f_instance, f_position), prior_g).is_none());
                assert_eq!(owners.remove(&f_owner), Some(f_instance));
                assert!(owners.insert(f_snapshot, f_instance).is_none());
                replay_snapshot_choices(&table, f_snapshot, &mut choices);
                assert!(selected(&table, g_input.condition(), &choices));
                next_instance += 1;
                let g_instance = next_instance;
                assert!(instance_nodes.insert(g_instance, g_node).is_none());
                assert!(owners.insert(g_owner, g_instance).is_none());
                let prior_f = owners.remove(&f_snapshot).unwrap();
                assert!(captured.insert((g_instance, g_position), prior_f).is_none());
                assert_eq!(owners.remove(&g_owner), Some(g_instance));
                assert!(owners.insert(g_snapshot, g_instance).is_none());
                replay_snapshot_choices(&table, g_snapshot, &mut choices);
                for owner in [
                    initial_owner,
                    f_header.owner(),
                    g_header.owner(),
                    f_owner,
                    f_snapshot,
                    g_owner,
                    g_snapshot,
                ] {
                    assert!(active_root_actions(owner, &choices).is_empty(), "{jump}");
                }
                if jump == "break" {
                    let Some(CleanupCondition::Choice { selector, branches }) =
                        table.get(jump_edge.condition())
                    else {
                        panic!("break edge must depend on the current flag")
                    };
                    assert_eq!(
                        branches,
                        &[CleanupConditionId::ALWAYS, CleanupConditionId::NEVER]
                    );
                    let control = table.selector(*selector).unwrap().control().unwrap();
                    assert_eq!(
                        sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
                        Ok("if (flag) { break }")
                    );
                    let mut false_choices = choices.clone();
                    false_choices.insert(*selector, 1);
                    assert!(!selected(&table, jump_edge.condition(), &false_choices));
                    choices.insert(*selector, 0);
                } else {
                    assert_eq!(jump_edge.condition(), CleanupConditionId::ALWAYS);
                }
                let jump_root = active_root(jump_g, &owners, &instance_nodes, &choices);
                assert_eq!(jump_root.source(), g_snapshot, "{jump}");
                assert_eq!(jump_root.node(), g_node, "{jump}");
                assert_eq!(owners[&jump_root.source()], g_instance, "{jump}");
                let jump_values = replay_captured_edge(
                    &table,
                    graph,
                    phis,
                    jump_edge,
                    &owners,
                    &instance_nodes,
                    &captured,
                    &mut choices,
                );
                assert_eq!(jump_values.len(), 1);
                assert_eq!(jump_values[&jump_g.target()], (g_snapshot, g_instance));
                assert_eq!(
                    choices[&phi(f_symbol, boundary).availability_selector()],
                    0,
                    "{jump}"
                );
                for root in jump_target.root_origins() {
                    assert_eq!(
                        choices[&root.selector()],
                        usize::from(root.node() == g_node || root.node() == initial_node),
                        "{jump}"
                    );
                }
                assert_eq!(owners.remove(&jump_root.source()), Some(g_instance));
                assert!(owners.insert(jump_g.target(), g_instance).is_none());
                if jump == "continue" && round + 1 < rounds {
                    assert_eq!(jump_g.target(), g_header.owner());
                }
            }
            if jump == "continue" {
                let exhausted = binding(
                    incoming(IterationPhiIncomingKind::Exhaustion),
                    g_exit.owner(),
                );
                assert!(exhausted.capture_slots_to_clear().is_empty());
                assert!(exhausted.origins().is_empty());
                assert_eq!(exhausted.values()[0].source(), g_header.owner());
                let exhausted_root = active_root(exhausted, &owners, &instance_nodes, &choices);
                assert_eq!(exhausted_root.source(), g_header.owner());
                assert_eq!(exhausted_root.node(), g_node);
                let exit_values = replay_captured_edge(
                    &table,
                    graph,
                    phis,
                    incoming(IterationPhiIncomingKind::Exhaustion),
                    &owners,
                    &instance_nodes,
                    &captured,
                    &mut choices,
                );
                assert_eq!(exit_values.len(), 1);
                assert_eq!(exit_values[&g_exit.owner()].0, g_header.owner());
                let instance = owners.remove(&exhausted_root.source()).unwrap();
                assert_eq!(instance, exit_values[&g_exit.owner()].1);
                assert!(owners.insert(g_exit.owner(), instance).is_none());
            }
            let releases = steps
                .iter()
                .filter_map(|(point, action)| match action {
                    IterationCleanupAction::ReleaseClosureInstances {
                        layout: ClosureReleaseLayout::Iteration(released_statement),
                        root,
                    } if *released_statement == statement
                        && root.owner() == Some(g_exit.owner())
                        && root
                            .condition()
                            .is_none_or(|condition| selected(&table, condition, &choices)) =>
                    {
                        Some((*point, *root))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(release_point, release)] = releases.as_slice() else {
                panic!("{jump} exit must have exactly one active instance release")
            };
            let final_call = parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, node)| (sources.slice(node.span()) == Ok("g()")).then_some(id))
                .last()
                .unwrap();
            assert_eq!(*release_point, DropPoint::CallReturn(final_call));
            assert_eq!(
                active_root_actions(g_exit.owner(), &choices),
                vec![(*release_point, true)]
            );
            assert_eq!(release.target(), DropTarget::Named(g_symbol));
            let chain_owners = phis
                .iter()
                .map(|phi| phi.owner())
                .chain([initial_owner, f_owner, f_snapshot, g_owner, g_snapshot])
                .collect::<BTreeSet<_>>();
            let active_chain_actions = steps
                .iter()
                .filter_map(|(point, action)| {
                    let (fact, instance_release) = match action {
                        IterationCleanupAction::Drop(fact) => (fact, false),
                        IterationCleanupAction::ReleaseClosureInstances { root, .. } => {
                            (root, true)
                        }
                        _ => return None,
                    };
                    let from_chain = fact
                        .owner()
                        .is_some_and(|owner| chain_owners.contains(&owner))
                        || fact.instance_address().is_some_and(|address| {
                            chain_owners.contains(&table.instance_address(address).unwrap().root())
                        })
                        || (*point == *release_point
                            && fact.target() == DropTarget::Named(g_symbol));
                    (from_chain
                        && fact
                            .condition()
                            .is_none_or(|condition| selected(&table, condition, &choices)))
                    .then_some((*point, instance_release, *fact))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                active_chain_actions,
                vec![(*release_point, true, *release)],
                "{jump} must release the formed chain only from its exit root"
            );
            let root = owners.remove(&release.owner().unwrap()).unwrap();
            let released =
                replay_owned_closure_release(graph, &instance_nodes, &mut captured, root);
            assert_eq!(released, (1..=next_instance).collect::<Vec<_>>(), "{jump}");
            assert!(owners.is_empty() && captured.is_empty(), "{jump}");
        }
    }

    #[test]
    fn recursive_return_releases_the_formed_chain_once() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-return.ko",
                "fun run(flags: List<Boolean>) { var f: move () -> Unit = move {}\nfor (flag in flags) { f = move { f() }\nif (flag) { return } }\nval used = f() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some());
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let graph = &planner.loop_capture_graphs[&statement.index()];
        let recursive_node = graph
            .nodes()
            .iter()
            .position(|node| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                ) == Ok("move { f() }")
            })
            .unwrap();
        let recursive = graph.nodes()[recursive_node].closure();
        let formed = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *closure == recursive =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        let (slot, input) = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == formed => Some((*target, *input)),
                _ => None,
            })
            .unwrap();
        let header = planner.loop_phis[&statement.index()]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
            .unwrap();
        assert_eq!(input.value(), CleanupCaptureValue::Owner(header.owner()));
        let entry = planner.loop_phi_incomings[&statement.index()]
            .iter()
            .find(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
        let root = entry
            .bindings()
            .iter()
            .find(|binding| binding.target() == header.owner())
            .unwrap();
        let [initial] = root.root_sources() else {
            panic!("entry must carry the initial environment")
        };
        let return_expression = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()) == Ok("return")).then_some(id))
            .unwrap();
        let release_point = DropPoint::ControlTransfer(return_expression);
        let releases = planner
            .cleanup
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(owner),
                    root,
                } if *point == release_point && *owner == statement => Some(*root),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [release] = releases.as_slice() else {
            panic!("return must release exactly one recursive root: {releases:?}")
        };
        // 形成事实的 owner 标识新环境；return 从已提交的具名 f 交付当次句柄。
        assert_eq!(release.owner(), Some(formed));
        assert_eq!(release.target(), DropTarget::Named(header.symbol()));
        let snapshot = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                    if *value == recursive =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        let mut choices = BTreeMap::new();
        let mut owners = BTreeMap::from([(initial.source(), 1_usize)]);
        let mut nodes = BTreeMap::from([(1, initial.node())]);
        let mut captured = BTreeMap::new();
        let (source, old) = replay_captured_edge_presence(
            &planner.conditions,
            graph,
            header,
            entry,
            header.owner(),
            &owners,
            &nodes,
            &captured,
            &mut choices,
        );
        assert_eq!(source, initial.source());
        assert_eq!(owners.remove(&source), Some(old));
        assert!(owners.insert(header.owner(), old).is_none());
        let formation = planner
            .cleanup
            .iter()
            .filter(|(point, action)| {
                *point == DropPoint::AfterExpression(recursive)
                    && matches!(
                        action,
                        IterationCleanupAction::CreateClosureOwner { .. }
                            | IterationCleanupAction::SaveClosureCapture { .. }
                            | IterationCleanupAction::SaveOwnerSnapshot { .. }
                            | IterationCleanupAction::CommitOwnerSnapshot { .. }
                    )
            })
            .map(|(_, action)| *action)
            .collect::<Vec<_>>();
        assert_eq!(formation.len(), 4);
        for action in formation {
            match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure } => {
                    assert_eq!((owner, closure), (formed, recursive));
                    assert!(owners.insert(owner, 2).is_none());
                    assert!(nodes.insert(2, recursive_node).is_none());
                }
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input: saved,
                } => {
                    assert_eq!((owner, target, saved), (formed, slot, input));
                    assert!(selected(&planner.conditions, saved.condition(), &choices));
                    let CleanupCaptureValue::Owner(source) = saved.value() else {
                        panic!("recursive capture must read the old header instance")
                    };
                    let prior = owners.remove(&source).unwrap();
                    let position = planner
                        .conditions
                        .capture_slot_value(target)
                        .unwrap()
                        .position();
                    assert!(captured.insert((owners[&owner], position), prior).is_none());
                }
                IterationCleanupAction::SaveOwnerSnapshot {
                    owner,
                    value,
                    condition,
                } => {
                    assert_eq!((owner, value), (snapshot, recursive));
                    assert!(
                        condition
                            .is_none_or(|guard| { selected(&planner.conditions, guard, &choices) })
                    );
                    let saved = planner.conditions.owner_snapshot(owner).unwrap();
                    let [source] = saved.value_inputs() else {
                        panic!("snapshot must transport the formed instance")
                    };
                    assert_eq!(source.owner(), formed);
                    assert!(selected(&planner.conditions, source.condition(), &choices));
                    let instance = owners.remove(&source.owner()).unwrap();
                    assert!(owners.insert(owner, instance).is_none());
                    replay_snapshot_choices(&planner.conditions, owner, &mut choices);
                }
                IterationCleanupAction::CommitOwnerSnapshot { owner, target } => {
                    assert_eq!((owner, target), (snapshot, header.symbol()));
                    let instance = owners.remove(&owner).unwrap();
                    assert!(owners.insert(header.owner(), instance).is_none());
                }
                _ => unreachable!("formation action filter"),
            }
        }
        let chain_owners = [initial.source(), header.owner(), formed, snapshot];
        assert!(
            !planner.cleanup.iter().any(|(point, action)| {
                if *point != DropPoint::AfterExpression(recursive) {
                    return false;
                }
                let fact = match action {
                    IterationCleanupAction::Drop(fact)
                    | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => fact,
                    _ => return false,
                };
                fact.owner()
                    .is_some_and(|owner| chain_owners.contains(&owner))
                    && fact
                        .condition()
                        .is_none_or(|guard| selected(&planner.conditions, guard, &choices))
            }),
            "forming the new environment must not release its old captured instance"
        );
        let return_control = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                (sources.slice(node.span()) == Ok("if (flag) { return }")).then_some(id)
            })
            .unwrap();
        let mut probe = planner.conditions.clone();
        let condition = probe
            .branch(
                return_control,
                parsed
                    .ast()
                    .expressions()
                    .get(return_control)
                    .unwrap()
                    .span(),
                2,
                0,
            )
            .unwrap();
        let Some(CleanupCondition::Choice { selector, .. }) = probe.get(condition) else {
            panic!("return branch must have a saved control selector")
        };
        choices.insert(*selector, 0);
        assert!(selected(
            &planner.conditions,
            release.condition().unwrap_or(CleanupConditionId::ALWAYS),
            &choices
        ));
        let active_releases = planner
            .cleanup
            .iter()
            .filter_map(|(point, action)| {
                if *point != release_point {
                    return None;
                }
                let fact = match action {
                    IterationCleanupAction::Drop(fact)
                    | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => fact,
                    _ => return None,
                };
                let from_chain = fact
                    .owner()
                    .is_some_and(|owner| chain_owners.contains(&owner))
                    || fact.instance_address().is_some_and(|address| {
                        chain_owners
                            .contains(&planner.conditions.instance_address(address).unwrap().root())
                    });
                (from_chain
                    && fact
                        .condition()
                        .is_none_or(|guard| selected(&planner.conditions, guard, &choices)))
                .then_some(*action)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            active_releases,
            [IterationCleanupAction::ReleaseClosureInstances {
                layout: ClosureReleaseLayout::Iteration(statement),
                root: *release,
            }]
        );
        assert_eq!(owners.remove(&header.owner()), Some(2));
        assert!(owners.is_empty());
        assert_eq!(
            replay_owned_closure_release(graph, &nodes, &mut captured, 2),
            [1, 2]
        );
        assert!(captured.is_empty());
    }

    #[test]
    fn recursive_capture_jump_edges_carry_the_new_root_instance() {
        for (jump, boundary) in [
            ("continue", IterationPhiBoundary::Header),
            ("break", IterationPhiBoundary::Exit),
        ] {
            let mut sources = SourceMap::new();
            let body = if jump == "break" {
                "if (flag) { break }"
            } else {
                "continue"
            };
            let source = sources
                .add_source(
                    "recursive-jump.ko",
                    format!("fun run(flags: List<Boolean>) {{ var f: move () -> Unit = move {{}}\nfor (flag in flags) {{ f = move {{ f() }}\n{body} }}\nval used = f() }}"),
                )
                .unwrap();
            let parsed =
                crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                    .unwrap();
            assert!(parsed.diagnostics().is_empty(), "{jump}");
            let (names, types) = crate::type_checking::standard_environments();
            let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
            let typed =
                crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty(), "{jump}");
            let mut checker =
                super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
            let capture_liveness = super::super::capture_liveness(&checker).unwrap();
            checker.expression_live_after = capture_liveness.expression_after;
            checker.statement_live_after = capture_liveness.statement_after;
            let mut state = super::super::super::State::default();
            for &root in parsed.roots() {
                checker.check_item(root, &mut state).unwrap();
            }
            assert!(checker.diagnostics.is_empty(), "{jump}");
            let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
            let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
            let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
            for &root in parsed.roots() {
                planner.item(root).unwrap();
            }
            assert!(planner.recursive_capture_phi.is_some(), "{jump}");
            let statement = checker
                .iterations
                .values()
                .next()
                .unwrap()
                .descriptor()
                .statement();
            let candidate = planner.into_candidate_facts();
            let [plan] = candidate.iterations.as_slice() else {
                panic!("one loop plan must be assembled for {jump}")
            };
            assert_eq!(plan.descriptor().statement(), statement);
            let table = &candidate.cleanup_conditions;
            let steps = &candidate.cleanup_steps;
            let graph = plan.capture_graph();
            let header = plan
                .closure_phis()
                .iter()
                .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
                .unwrap();
            let target = plan
                .closure_phis()
                .iter()
                .find(|phi| phi.boundary() == boundary)
                .unwrap();
            let edge = plan
                .closure_phi_incomings()
                .iter()
                .find(|incoming| match (jump, incoming.kind()) {
                    ("continue", IterationPhiIncomingKind::Continue(_))
                    | ("break", IterationPhiIncomingKind::Break(_)) => true,
                    _ => false,
                })
                .unwrap();
            assert_eq!(edge.boundary(), boundary, "{jump}");
            assert_ne!(edge.condition(), CleanupConditionId::NEVER, "{jump}");
            if jump == "continue" {
                assert_eq!(edge.condition(), CleanupConditionId::ALWAYS);
            } else {
                let Some(CleanupCondition::Choice { selector, branches }) =
                    table.get(edge.condition())
                else {
                    panic!("break must be guarded by the current flag")
                };
                assert_eq!(
                    branches,
                    &[CleanupConditionId::ALWAYS, CleanupConditionId::NEVER]
                );
                let choice = table.selector(*selector).unwrap();
                let control = choice.control().unwrap();
                assert_eq!(
                    sources
                        .slice(parsed.ast().expressions().get(control).unwrap().span())
                        .unwrap(),
                    "if (flag) { break }"
                );
            }
            let input = edge
                .bindings()
                .iter()
                .find(|input| input.target() == target.owner())
                .unwrap();
            assert!(input.capture_slots_to_clear().is_empty(), "{jump}");
            assert_eq!(
                input.availability_selector(),
                target.availability_selector(),
                "{jump}"
            );
            assert_eq!(input.available_when(), edge.condition(), "{jump}");
            assert_eq!(input.values().len(), 1, "{jump}");
            assert_eq!(input.values()[0].condition(), edge.condition(), "{jump}");
            let snapshot = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                        if *target == header.symbol() =>
                    {
                        Some(*owner)
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(input.values()[0].source(), snapshot, "{jump}");

            // 回放实际形成/捕获/快照动作：continue 的第二轮必须捕获第一轮实例，
            // break 则只把本轮新根交给 exit；两条边都不能把后代改指当前 phi。
            let entry = plan
                .closure_phi_incomings()
                .iter()
                .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
                .unwrap();
            let entry = entry
                .bindings()
                .iter()
                .find(|binding| binding.target() == header.owner())
                .unwrap();
            let active_root = |binding: &IterationPhiIncomingBinding,
                               choices: &BTreeMap<_, _>,
                               owners: &BTreeMap<_, usize>,
                               nodes: &BTreeMap<usize, usize>| {
                let values = binding
                    .values()
                    .iter()
                    .filter(|value| selected(&table, value.condition(), choices))
                    .collect::<Vec<_>>();
                let [value] = values.as_slice() else {
                    panic!("{jump} must carry exactly one owner value")
                };
                let roots = binding
                    .root_sources()
                    .iter()
                    .filter(|root| {
                        root.source() == value.source()
                            && root.node() == nodes[&owners[&value.source()]]
                            && selected(&table, root.condition(), choices)
                    })
                    .collect::<Vec<_>>();
                let [root] = roots.as_slice() else {
                    panic!("{jump} must carry exactly one formed root")
                };
                **root
            };
            let initial_owner = entry.values()[0].source();
            fn unique_action_index(
                steps: &[(DropPoint, IterationCleanupAction)],
                matches: impl Fn(&IterationCleanupAction) -> bool,
            ) -> usize {
                let found = steps
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (_, action))| matches(action).then_some(index))
                    .collect::<Vec<_>>();
                let [index] = found.as_slice() else {
                    panic!("one matching formation action must exist")
                };
                *index
            }
            let mut formed = BTreeMap::new();
            for (_, action) in steps {
                if let IterationCleanupAction::CreateClosureOwner { owner, closure } = action {
                    assert!(formed.insert(*owner, *closure).is_none());
                }
            }
            let captures = steps
                .iter()
                .filter_map(|(_, action)| match action {
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if input.value() == CleanupCaptureValue::Owner(header.owner()) => {
                        Some((*owner, *target, *input))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [capture] = captures.as_slice() else {
                panic!("one formed environment must capture the previous header")
            };
            let capture = *capture;
            assert_eq!(capture.2.condition(), header.availability_condition());
            assert_eq!(capture.2.mode(), ClosureCaptureMode::Owned);
            assert_eq!(capture.2.effect(), ClosureCaptureEffect::Move);
            let slot = table.capture_slot_value(capture.1).unwrap();
            assert_eq!(slot.environment(), capture.0);
            assert_eq!(slot.source(), capture.2.source());
            let position = slot.position();
            let snapshot_input = table.owner_snapshot(snapshot).unwrap();
            assert_eq!(snapshot_input.capture_inputs().len(), 1);
            assert_eq!(snapshot_input.capture_inputs()[0].owner(), capture.0);
            let create_at = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::CreateClosureOwner { owner, .. }
                    if *owner == capture.0)
            });
            let capture_at = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::SaveClosureCapture { owner, .. }
                    if *owner == capture.0)
            });
            let snapshot_at = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner, .. }
                    if *owner == snapshot)
            });
            let commit_at = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, .. }
                    if *owner == snapshot)
            });
            assert_eq!(
                steps[commit_at].1,
                IterationCleanupAction::CommitOwnerSnapshot {
                    owner: snapshot,
                    target: header.symbol(),
                }
            );
            assert!(create_at < capture_at && capture_at < snapshot_at && snapshot_at < commit_at);
            assert_eq!(steps[create_at].0, steps[capture_at].0);
            let IterationCleanupAction::SaveOwnerSnapshot {
                condition,
                owner,
                value,
            } = steps[snapshot_at].1
            else {
                unreachable!()
            };
            assert_eq!(owner, snapshot);
            assert_eq!(value, snapshot_input.value());
            assert!(condition.is_none_or(|guard| guard == CleanupConditionId::ALWAYS));
            assert_eq!(value, formed[&capture.0]);
            assert_eq!(
                sources
                    .slice(parsed.ast().expressions().get(value).unwrap().span())
                    .unwrap(),
                "move { f() }"
            );
            for index in [create_at, capture_at, snapshot_at, commit_at] {
                assert_eq!(steps[index].0, DropPoint::AfterExpression(value));
            }
            let exit = plan
                .closure_phis()
                .iter()
                .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
                .unwrap();
            let root_drop = candidate
                .drops
                .iter()
                .find(|fact| fact.target() == DropTarget::Named(header.symbol()))
                .unwrap();
            assert_eq!(root_drop.owner(), Some(exit.owner()));
            let releases = steps
                .iter()
                .filter_map(|(point, action)| match action {
                    IterationCleanupAction::ReleaseClosureInstances {
                        layout: ClosureReleaseLayout::Iteration(release_statement),
                        root,
                    } if *release_statement == statement && *root == *root_drop => {
                        Some((*point, *root))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(release_point, release_root)] = releases.as_slice() else {
                panic!("{jump} must have exactly one exit instance release")
            };
            let final_call = parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, node)| (sources.slice(node.span()) == Ok("f()")).then_some(id))
                .last()
                .unwrap();
            assert_eq!(*release_point, DropPoint::CallReturn(final_call));
            let chain_owners = plan
                .closure_phis()
                .iter()
                .map(|phi| phi.owner())
                .chain([initial_owner, capture.0, snapshot])
                .collect::<BTreeSet<_>>();
            let active_chain_actions = |choices: &BTreeMap<_, _>| {
                steps
                    .iter()
                    .filter_map(|(point, action)| {
                        let (fact, instance_release) = match action {
                            IterationCleanupAction::Drop(fact) => (fact, false),
                            IterationCleanupAction::ReleaseClosureInstances { root, .. } => {
                                (root, true)
                            }
                            _ => return None,
                        };
                        let from_chain = fact
                            .owner()
                            .is_some_and(|owner| chain_owners.contains(&owner))
                            || fact.instance_address().is_some_and(|address| {
                                chain_owners
                                    .contains(&table.instance_address(address).unwrap().root())
                            })
                            || (*point == *release_point
                                && fact.target() == DropTarget::Named(header.symbol()));
                        (from_chain
                            && fact
                                .condition()
                                .is_none_or(|guard| selected(&table, guard, choices)))
                        .then_some((*point, instance_release, *fact))
                    })
                    .collect::<Vec<_>>()
            };
            let expected_release = (*release_point, true, *release_root);
            let check_before_exit = |choices: &BTreeMap<_, _>| {
                let actions = active_chain_actions(choices);
                assert!(
                    actions.is_empty() || actions == vec![expected_release],
                    "{jump} must not release a captured instance before the exit: {actions:?}"
                );
            };
            let mut owners = BTreeMap::from([(initial_owner, 1_usize)]);
            let mut instance_closures = BTreeMap::from([(1_usize, formed[&initial_owner])]);
            let mut instance_nodes = BTreeMap::from([(
                1_usize,
                graph
                    .nodes()
                    .iter()
                    .position(|node| node.closure() == formed[&initial_owner])
                    .unwrap(),
            )]);
            let mut captured = BTreeMap::new();
            let mut choices = table
                .nodes()
                .iter()
                .filter_map(|node| match node {
                    CleanupCondition::Choice { selector, .. } => Some((*selector, 0)),
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>();
            assert_ne!(formed[&initial_owner], formed[&capture.0]);
            let entry_root = active_root(entry, &choices, &owners, &instance_nodes);
            assert_eq!(entry_root.source(), initial_owner);
            assert_eq!(
                graph.nodes()[entry_root.node()].closure(),
                formed[&initial_owner]
            );
            let entry_edge = plan
                .closure_phi_incomings()
                .iter()
                .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
                .unwrap();
            let entry_values = replay_captured_edge(
                &table,
                graph,
                &plan.closure_phis(),
                entry_edge,
                &owners,
                &instance_nodes,
                &captured,
                &mut choices,
            );
            assert_eq!(entry_values[&entry.target()], (entry_root.source(), 1));
            let seed = owners.remove(&entry_root.source()).unwrap();
            owners.insert(entry.target(), seed);
            for root in header.root_origins() {
                assert_eq!(
                    choices[&root.selector()],
                    usize::from(root.node() == entry_root.node()),
                    "{jump}"
                );
            }
            let rounds = if jump == "continue" { 2 } else { 1 };
            for instance in 2..=rounds + 1 {
                assert!(selected(&table, capture.2.condition(), &choices));
                assert!(
                    instance_closures
                        .insert(instance, formed[&capture.0])
                        .is_none()
                );
                assert!(
                    instance_nodes
                        .insert(
                            instance,
                            graph
                                .nodes()
                                .iter()
                                .position(|node| node.closure() == formed[&capture.0])
                                .unwrap(),
                        )
                        .is_none()
                );
                let old = owners.remove(&header.owner()).unwrap();
                assert!(captured.insert((instance, position), old).is_none());
                owners.insert(capture.0, instance);
                let formed_value = owners
                    .remove(&snapshot_input.capture_inputs()[0].owner())
                    .unwrap();
                owners.insert(snapshot, formed_value);
                check_before_exit(&choices);
                replay_snapshot_choices(&table, snapshot, &mut choices);
                check_before_exit(&choices);
                let incoming = if instance == rounds + 1 {
                    input
                } else {
                    plan.closure_phi_incomings()
                        .iter()
                        .find(|incoming| {
                            matches!(incoming.kind(), IterationPhiIncomingKind::Continue(_))
                        })
                        .unwrap()
                        .bindings()
                        .iter()
                        .find(|binding| binding.target() == header.owner())
                        .unwrap()
                };
                let root = active_root(incoming, &choices, &owners, &instance_nodes);
                assert_eq!(root.source(), snapshot);
                assert_eq!(graph.nodes()[root.node()].closure(), formed[&capture.0]);
                let jump_edge = plan
                    .closure_phi_incomings()
                    .iter()
                    .find(|edge| match (jump, edge.kind()) {
                        ("continue", IterationPhiIncomingKind::Continue(_))
                        | ("break", IterationPhiIncomingKind::Break(_)) => true,
                        _ => false,
                    })
                    .unwrap();
                let jump_values = replay_captured_edge(
                    &table,
                    graph,
                    &plan.closure_phis(),
                    jump_edge,
                    &owners,
                    &instance_nodes,
                    &captured,
                    &mut choices,
                );
                assert_eq!(jump_values[&incoming.target()], (root.source(), instance));
                let carried = owners.remove(&root.source()).unwrap();
                let target_phi = plan
                    .closure_phis()
                    .iter()
                    .find(|phi| phi.owner() == incoming.target())
                    .unwrap();
                let reached_nodes = BTreeSet::from([instance_nodes[&1], instance_nodes[&instance]]);
                for target_root in target_phi.root_origins() {
                    assert_eq!(
                        choices[&target_root.selector()],
                        usize::from(reached_nodes.contains(&target_root.node())),
                        "{jump}"
                    );
                }
                assert!(owners.insert(incoming.target(), carried).is_none());
            }
            if jump == "continue" {
                let exhaustion = plan
                    .closure_phi_incomings()
                    .iter()
                    .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
                    .unwrap();
                let forwarded = exhaustion
                    .bindings()
                    .iter()
                    .find(|binding| binding.target() == exit.owner())
                    .unwrap();
                assert_eq!(forwarded.values()[0].source(), header.owner());
                let forwarded_root = active_root(forwarded, &choices, &owners, &instance_nodes);
                assert_eq!(forwarded_root.source(), header.owner());
                assert_eq!(
                    graph.nodes()[forwarded_root.node()].closure(),
                    formed[&capture.0]
                );
                let exit_values = replay_captured_edge(
                    &table,
                    graph,
                    &plan.closure_phis(),
                    exhaustion,
                    &owners,
                    &instance_nodes,
                    &captured,
                    &mut choices,
                );
                assert_eq!(
                    exit_values[&forwarded.target()],
                    (forwarded_root.source(), rounds + 1)
                );
                let root = owners.remove(&forwarded_root.source()).unwrap();
                let reached_nodes = BTreeSet::from([instance_nodes[&1], instance_nodes[&root]]);
                for target_root in exit.root_origins() {
                    assert_eq!(
                        choices[&target_root.selector()],
                        usize::from(reached_nodes.contains(&target_root.node())),
                        "{jump}"
                    );
                }
                owners.insert(forwarded.target(), root);
            }
            assert_eq!(active_chain_actions(&choices), vec![expected_release]);
            assert_eq!(instance_nodes.len(), instance_closures.len());
            let root_instance = owners.remove(&release_root.owner().unwrap()).unwrap();
            let released =
                replay_owned_closure_release(graph, &instance_nodes, &mut captured, root_instance);
            assert_eq!(released, (1..=rounds + 1).collect::<Vec<_>>(), "{jump}");
            assert!(owners.is_empty() && captured.is_empty(), "{jump}");
        }
    }

    #[test]
    fn recursive_graph_keeps_independent_unexpanded_closure_roots_owned() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "mixed-recursive-roots.ko",
                "fun run(flags: List<Int>) { var f: move () -> Unit = move {}
val first_leaf: move () -> Unit = move {}
val second_leaf: move () -> Unit = move {}
var g: move () -> Unit = move { val a = first_leaf()
val b = second_leaf() }
for (_ in flags) { f = move { f() } }
val first = f()
val second = g() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some());
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let graph = &planner.loop_capture_graphs[&statement.index()];
        let phis = &planner.loop_phis[&statement.index()];
        let headers = phis
            .iter()
            .filter(|phi| phi.boundary() == IterationPhiBoundary::Header)
            .filter(|phi| !phi.root_nodes().is_empty())
            .collect::<Vec<_>>();
        assert_eq!(headers.len(), 2);
        let independent = headers
            .iter()
            .find(|phi| {
                phi.root_nodes().len() == 1
                    && sources
                        .slice(
                            parsed
                                .ast()
                                .expressions()
                                .get(graph.nodes()[phi.root_nodes()[0]].closure())
                                .unwrap()
                                .span(),
                        )
                        .unwrap()
                        == "move { val a = first_leaf()\nval b = second_leaf() }"
            })
            .unwrap();
        let node = &graph.nodes()[independent.root_nodes()[0]];
        assert_eq!(node.sources().len(), 2);
        let formed_owner = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *closure == node.closure() =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        let relevant_nodes = BTreeSet::from([
            independent.root_nodes()[0],
            node.sources()[0].captured()[0],
            node.sources()[1].captured()[0],
        ]);
        assert_eq!(relevant_nodes.len(), 3);
        assert_eq!(independent.origins().len(), relevant_nodes.len());
        let mut instance_nodes = BTreeMap::new();
        let mut owner_instances = BTreeMap::new();
        let mut saved_children = BTreeMap::new();
        for (index, captured) in node.sources().iter().enumerate() {
            assert_eq!(captured.position(), index);
            assert_eq!(
                sources.slice(captured.capture().reference_span()).unwrap(),
                ["first_leaf", "second_leaf"][index]
            );
            assert_eq!(captured.capture().mode(), ClosureCaptureMode::Owned);
            assert_eq!(captured.capture().effect(), ClosureCaptureEffect::Move);
            assert_eq!(captured.captured().len(), 1);
            assert!(graph.nodes()[captured.captured()[0]].sources().is_empty());
        }
        for (point, action) in &planner.cleanup {
            match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure } => {
                    let Some(node_index) = graph
                        .nodes()
                        .iter()
                        .position(|node| node.closure() == *closure)
                    else {
                        continue;
                    };
                    if !relevant_nodes.contains(&node_index) {
                        continue;
                    }
                    assert_eq!(*point, DropPoint::AfterExpression(*closure));
                    let instance = instance_nodes.len() + 1;
                    assert!(instance_nodes.insert(instance, node_index).is_none());
                    assert!(owner_instances.insert(*owner, instance).is_none());
                }
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == formed_owner => {
                    assert_eq!(*point, DropPoint::AfterExpression(node.closure()));
                    assert_eq!(input.condition(), CleanupConditionId::ALWAYS);
                    let slot = planner.conditions.capture_slot_value(*target).unwrap();
                    assert_eq!(slot.environment(), formed_owner);
                    let expected = &node.sources()[slot.position()];
                    assert_eq!(input.source(), expected.capture().source());
                    assert_eq!(input.mode(), ClosureCaptureMode::Owned);
                    assert_eq!(input.effect(), ClosureCaptureEffect::Move);
                    let CleanupCaptureValue::Owner(source_owner) = input.value() else {
                        panic!("owned child must come from its formed owner")
                    };
                    let parent = owner_instances[owner];
                    let child = owner_instances.remove(&source_owner).unwrap();
                    assert!(expected.captured().contains(&instance_nodes[&child]));
                    assert!(
                        saved_children
                            .insert((parent, slot.position()), child)
                            .is_none()
                    );
                }
                _ => {}
            }
        }
        assert_eq!(instance_nodes.len(), 3);
        assert_eq!(saved_children.len(), 2);
        let g_instance = owner_instances[&formed_owner];
        assert_eq!(owner_instances.len(), 1);
        for header in &headers {
            let exit = phis
                .iter()
                .find(|phi| {
                    phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == header.symbol()
                })
                .unwrap();
            let root = planner
                .facts
                .iter()
                .find(|fact| {
                    fact.target() == DropTarget::Named(header.symbol())
                        && fact.owner() == Some(exit.owner())
                })
                .unwrap();
            if header.symbol() == independent.symbol() {
                assert!(planner.cleanup.iter().any(|(_, action)| matches!(action,
                    IterationCleanupAction::Drop(fact) if *fact == *root)));
            } else {
                assert!(planner.cleanup.iter().any(|(_, action)| matches!(action,
                    IterationCleanupAction::ReleaseClosureInstances { layout: ClosureReleaseLayout::Iteration(release_statement), root: released }
                        if *release_statement == statement && *released == *root)));
            }
        }
        let incomings = &planner.loop_phi_incomings[&statement.index()];
        let entry = incomings
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap()
            .bindings()
            .iter()
            .find(|binding| binding.target() == independent.owner())
            .unwrap();
        assert_eq!(entry.values().len(), 1);
        assert_eq!(entry.values()[0].source(), formed_owner);
        assert!(
            !entry.origins().is_empty(),
            "independent g keeps its child sources"
        );
        let exhausted = incomings
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
            .unwrap()
            .bindings()
            .iter()
            .find(|binding| {
                phis.iter().any(|phi| {
                    phi.boundary() == IterationPhiBoundary::Exit
                        && phi.symbol() == independent.symbol()
                        && phi.owner() == binding.target()
                })
            })
            .unwrap();
        assert_eq!(exhausted.values().len(), 1);
        assert_eq!(exhausted.values()[0].source(), independent.owner());
        let release_root = planner
            .facts
            .iter()
            .find(|fact| {
                fact.owner() == Some(exhausted.target())
                    && matches!(fact.target(), DropTarget::Named(_))
            })
            .unwrap();
        let flat = planner
            .cleanup
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::Drop(fact) if *point == release_root.point() => (fact
                    == release_root
                    || fact.instance_address().is_some_and(|address| {
                        planner.conditions.instance_address(address).unwrap().root()
                            == exhausted.target()
                    }))
                .then_some(*fact),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(flat.len(), 3);
        assert_eq!(flat.last(), Some(release_root));
        assert_eq!(
            flat[..2]
                .iter()
                .map(|fact| {
                    planner
                        .conditions
                        .capture_slot_value(fact.capture_slot().unwrap())
                        .unwrap()
                        .position()
                })
                .collect::<Vec<_>>(),
            [1, 0]
        );
        let expected_release = [
            saved_children[&(g_instance, 1)],
            saved_children[&(g_instance, 0)],
            g_instance,
        ];
        let mut values = BTreeMap::from([(entry.values()[0].source(), g_instance)]);
        let formed = values.remove(&entry.values()[0].source()).unwrap();
        values.insert(entry.target(), formed);
        let forwarded = values.remove(&exhausted.values()[0].source()).unwrap();
        values.insert(exhausted.target(), forwarded);
        let root_instance = values.remove(&release_root.owner().unwrap()).unwrap();
        assert_eq!(root_instance, g_instance);
        assert_eq!(
            replay_owned_closure_release(
                &planner.loop_capture_graphs[&statement.index()],
                &instance_nodes,
                &mut saved_children,
                root_instance,
            ),
            expected_release
        );
        assert!(saved_children.is_empty() && values.is_empty());
    }

    #[test]
    fn recursive_loop_keeps_independent_shared_capture_loan_end() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-with-shared-root.ko",
                "fun read(xs: List<Int>) {}\nfun run(xs: List<Int>, flags: List<Int>) {
var f: move () -> Unit = move {}
var g: () -> Unit = { read(xs) }
for (_ in flags) { f = move { f() } }
val used = g()
val done = f() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some());
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let graph = &planner.loop_capture_graphs[&statement.index()];
        let independent = planner.loop_phis[&statement.index()]
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit
                    && phi.root_nodes().iter().any(|&node| {
                        sources.slice(
                            parsed
                                .ast()
                                .expressions()
                                .get(graph.nodes()[node].closure())
                                .unwrap()
                                .span(),
                        ) == Ok("{ read(xs) }")
                    })
            })
            .unwrap();
        let root = planner
            .facts
            .iter()
            .find(|fact| {
                fact.owner() == Some(independent.owner())
                    && matches!(fact.target(), DropTarget::Named(_))
            })
            .unwrap();
        assert!(planner.cleanup.iter().any(|(point, action)| {
            *point == root.point()
                && matches!(action, IterationCleanupAction::Drop(fact) if fact == root)
        }));
        let loan_ends = planner
            .cleanup
            .iter()
            .filter(|(point, action)| {
                *point == root.point()
                    && matches!(action, IterationCleanupAction::EndCaptureLoan {
                        instance_address,
                        closure,
                        ..
                    } if *closure == graph.nodes()[independent.root_nodes()[0]].closure()
                        && planner.conditions.instance_address(*instance_address).unwrap().root() == independent.owner())
            })
            .collect::<Vec<_>>();
        assert_eq!(loan_ends.len(), 1, "the one shared capture ends once");
    }

    #[test]
    fn recursive_release_layout_keeps_untracked_shared_capture() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-copyable-shared.ko",
                "fun read(n: Int) {}\nfun run(n: Int, flags: List<Int>) {
var f: move () -> Unit = move {}
for (_ in flags) {
    val g: () -> Unit = { read(n) }
    f = move { val old = f()\nval borrowed = g() }
}
val used = f() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let checked =
            crate::ownership_checking::check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert!(checked.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some());
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let graph = &planner.loop_capture_graphs[&statement.index()];
        let borrowed = graph
            .nodes()
            .iter()
            .find(|node| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                ) == Ok("{ read(n) }")
            })
            .unwrap();
        assert!(
            borrowed.sources().is_empty(),
            "Copyable capture has no phi source"
        );
        let [capture] = borrowed.release_captures() else {
            panic!("instance release must see the shared capture omitted by phi")
        };
        assert_eq!(
            checked.captures_of(borrowed.closure()).collect::<Vec<_>>(),
            [capture]
        );
        assert_eq!(capture.mode(), ClosureCaptureMode::Shared);
        assert_eq!(capture.effect(), ClosureCaptureEffect::Borrow);
        assert_eq!(sources.slice(capture.reference_span()), Ok("n"));
        let formed = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *closure == borrowed.closure() =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        let captures = planner
            .cleanup
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == formed => Some((*target, *input)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(slot, input)] = captures.as_slice() else {
            panic!("the formed child must save its one shared capture")
        };
        let layout = planner.conditions.capture_slot_value(*slot).unwrap();
        assert_eq!(layout.environment(), formed);
        assert_eq!(layout.position(), 0);
        assert_eq!(layout.source(), capture.source());
        assert_eq!(input.source(), capture.source());
        assert_eq!(input.value(), CleanupCaptureValue::Place(capture.source()));
        assert_eq!(
            (input.mode(), input.effect()),
            (capture.mode(), capture.effect())
        );
        let recursive_node = graph
            .nodes()
            .iter()
            .position(|node| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                ) == Ok("move { val old = f()\nval borrowed = g() }")
            })
            .unwrap();
        let borrowed_node = graph
            .nodes()
            .iter()
            .position(|node| node.closure() == borrowed.closure())
            .unwrap();
        let initial_node = graph
            .nodes()
            .iter()
            .position(|node| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                ) == Ok("move {}")
            })
            .unwrap();
        let recursive_closure = graph.nodes()[recursive_node].closure();
        let recursive_owner = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *closure == recursive_closure =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        let recursive_inputs = planner
            .cleanup
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == recursive_owner => Some((*target, *input)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(recursive_inputs.len(), 2);
        for (position, (slot, input)) in recursive_inputs.iter().enumerate() {
            let saved = planner.conditions.capture_slot_value(*slot).unwrap();
            assert_eq!(saved.position(), position);
            assert_eq!(saved.source(), input.source());
            assert_eq!(
                (input.mode(), input.effect()),
                (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
            );
        }
        let snapshot = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                    if *value == recursive_closure =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        let borrowed_snapshot = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                    if *value == borrowed.closure() =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        let phis = &planner.loop_phis[&statement.index()];
        let header = phis
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Header
                    && phi.root_nodes().contains(&recursive_node)
            })
            .unwrap();
        let exit = phis
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == header.symbol()
            })
            .unwrap();
        let incoming = |kind| {
            planner.loop_phi_incomings[&statement.index()]
                .iter()
                .find(|edge| edge.kind() == kind)
                .unwrap()
        };
        let binding = |kind, target| {
            incoming(kind)
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap()
        };
        let entry = binding(IterationPhiIncomingKind::Entry, header.owner());
        let backedge = binding(IterationPhiIncomingKind::Fallthrough, header.owner());
        let exhausted = binding(IterationPhiIncomingKind::Exhaustion, exit.owner());
        assert_eq!(entry.root_sources().len(), 1, "{:?}", entry.root_sources());
        assert_eq!(
            entry.root_sources()[0].condition(),
            CleanupConditionId::ALWAYS
        );
        assert_eq!(entry.values().len(), 1);
        assert_eq!(backedge.values().len(), 1);
        assert_eq!(exhausted.values().len(), 1);
        assert_eq!(backedge.values()[0].source(), snapshot);
        assert_eq!(
            recursive_inputs[0].1.value(),
            CleanupCaptureValue::Owner(header.owner())
        );
        assert_eq!(
            recursive_inputs[1].1.value(),
            CleanupCaptureValue::Owner(borrowed_snapshot)
        );
        let release = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(release_statement),
                    root,
                } if *release_statement == statement && root.owner() == Some(exit.owner()) => {
                    Some(*root)
                }
                _ => None,
            })
            .unwrap();
        assert!(!planner.cleanup.iter().any(|(_, action)| {
            matches!(action, IterationCleanupAction::EndCaptureLoan { closure, .. }
                if *closure == borrowed.closure())
        }));

        let active_root = |binding: &IterationPhiIncomingBinding,
                           choices: &BTreeMap<_, _>,
                           values: &BTreeMap<_, usize>,
                           nodes: &BTreeMap<usize, usize>| {
            let selected_values = binding
                .values()
                .iter()
                .filter(|value| selected(&planner.conditions, value.condition(), choices))
                .collect::<Vec<_>>();
            let [value] = selected_values.as_slice() else {
                panic!("one owner value must reach this edge")
            };
            let roots = binding
                .root_sources()
                .iter()
                .filter(|root| {
                    root.source() == value.source()
                        && root.node() == nodes[&values[&value.source()]]
                        && selected(&planner.conditions, root.condition(), choices)
                })
                .collect::<Vec<_>>();
            let [root] = roots.as_slice() else {
                panic!("one formed root must reach this edge")
            };
            **root
        };
        let initial_owner = entry.values()[0].source();
        assert!(planner.cleanup.iter().any(|(_, action)| matches!(
            action,
            IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *owner == initial_owner && *closure == graph.nodes()[initial_node].closure()
        )));
        let mut values = BTreeMap::from([(initial_owner, 1usize)]);
        let mut instance_nodes = BTreeMap::from([(1usize, initial_node)]);
        let mut owned_edges = BTreeMap::<(usize, usize), usize>::new();
        let mut shared_loans = BTreeMap::<(usize, usize), CleanupCaptureValue>::new();
        let mut choices = BTreeMap::new();
        let entry_values = replay_captured_edge(
            &planner.conditions,
            graph,
            phis,
            incoming(IterationPhiIncomingKind::Entry),
            &values,
            &instance_nodes,
            &owned_edges,
            &mut choices,
        );
        assert_eq!(entry_values[&header.owner()], (initial_owner, 1));
        assert_eq!(values.remove(&initial_owner), Some(1));
        values.insert(header.owner(), 1);
        let mut checkpoints = vec![(
            values.clone(),
            instance_nodes.clone(),
            owned_edges.clone(),
            shared_loans.clone(),
            choices.clone(),
        )];
        let mut formed_instances = Vec::new();
        for round in 0..2 {
            let child = round * 2 + 2;
            let parent = child + 1;
            assert!(
                choices.contains_key(&header.availability_selector()),
                "header {:?} choices {:?}",
                header.availability_selector(),
                choices
            );
            assert!(selected(&planner.conditions, input.condition(), &choices));
            values.insert(formed, child);
            instance_nodes.insert(child, borrowed_node);
            assert!(
                shared_loans
                    .insert((child, layout.position()), input.value())
                    .is_none()
            );
            let formed_child = values.remove(&formed).unwrap();
            values.insert(borrowed_snapshot, formed_child);
            // g 的结果快照复制旧 header 选择位，随后形成 f 时读取的是这份独立身份。
            replay_snapshot_choices(&planner.conditions, borrowed_snapshot, &mut choices);
            for (_, capture) in &recursive_inputs {
                assert!(selected(&planner.conditions, capture.condition(), &choices));
            }
            values.insert(recursive_owner, parent);
            instance_nodes.insert(parent, recursive_node);
            for (slot, capture) in &recursive_inputs {
                let CleanupCaptureValue::Owner(source) = capture.value() else {
                    panic!("owned child must read a formed owner")
                };
                let child_instance = values.remove(&source).unwrap();
                let position = planner
                    .conditions
                    .capture_slot_value(*slot)
                    .unwrap()
                    .position();
                assert!(
                    owned_edges
                        .insert((parent, position), child_instance)
                        .is_none()
                );
            }
            let formed_parent = values.remove(&recursive_owner).unwrap();
            values.insert(snapshot, formed_parent);
            replay_snapshot_choices(&planner.conditions, snapshot, &mut choices);
            let source = active_root(backedge, &choices, &values, &instance_nodes).source();
            let transported = replay_captured_edge(
                &planner.conditions,
                graph,
                phis,
                incoming(IterationPhiIncomingKind::Fallthrough),
                &values,
                &instance_nodes,
                &owned_edges,
                &mut choices,
            );
            assert_eq!(transported[&header.owner()], (source, parent));
            let forwarded = values.remove(&source).unwrap();
            assert_eq!(forwarded, parent);
            values.insert(header.owner(), forwarded);
            formed_instances.push((child, parent));
            checkpoints.push((
                values.clone(),
                instance_nodes.clone(),
                owned_edges.clone(),
                shared_loans.clone(),
                choices.clone(),
            ));
        }

        #[derive(Debug, PartialEq, Eq)]
        enum Released {
            Loan(usize, usize),
            Environment(usize),
        }
        enum Step {
            Enter(usize),
            EndLoan(usize, usize),
            Finish(usize),
        }
        let release_checkpoint = |(mut values, nodes, mut edges, mut loans, mut choices): (
            BTreeMap<CleanupOwnerValueId, usize>,
            BTreeMap<usize, usize>,
            BTreeMap<(usize, usize), usize>,
            BTreeMap<(usize, usize), CleanupCaptureValue>,
            BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
        )| {
            let source = active_root(exhausted, &choices, &values, &nodes).source();
            let transported = replay_captured_edge(
                &planner.conditions,
                graph,
                phis,
                incoming(IterationPhiIncomingKind::Exhaustion),
                &values,
                &nodes,
                &edges,
                &mut choices,
            );
            assert_eq!(transported[&exit.owner()].0, source);
            let forwarded = values.remove(&source).unwrap();
            values.insert(exit.owner(), forwarded);
            assert!(selected(
                &planner.conditions,
                release.condition().unwrap_or(CleanupConditionId::ALWAYS),
                &choices
            ));
            let root = values.remove(&release.owner().unwrap()).unwrap();
            let mut pending = vec![Step::Enter(root)];
            let mut visited = BTreeSet::new();
            let mut released = Vec::new();
            let mut remaining_loans = loans.len();
            while let Some(step) = pending.pop() {
                match step {
                    Step::Enter(instance) => {
                        assert!(visited.insert(instance), "an instance must release once");
                        pending.push(Step::Finish(instance));
                        let node = &graph.nodes()[nodes[&instance]];
                        for (position, capture) in checked.captures_of(node.closure()).enumerate() {
                            match (capture.mode(), capture.effect()) {
                                (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move) => {
                                    let child = edges.remove(&(instance, position)).unwrap();
                                    let source = node
                                        .sources()
                                        .iter()
                                        .find(|source| source.position() == position)
                                        .unwrap();
                                    assert!(source.captured().contains(&nodes[&child]));
                                    pending.push(Step::Enter(child));
                                }
                                (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow) => {
                                    pending.push(Step::EndLoan(instance, position));
                                }
                                other => panic!("unexpected capture in replay: {other:?}"),
                            }
                        }
                    }
                    Step::EndLoan(instance, position) => {
                        assert_eq!(loans.remove(&(instance, position)), Some(input.value()));
                        remaining_loans -= 1;
                        released.push(Released::Loan(instance, remaining_loans));
                    }
                    Step::Finish(instance) => released.push(Released::Environment(instance)),
                }
            }
            assert!(values.is_empty() && edges.is_empty() && loans.is_empty());
            assert_eq!(remaining_loans, 0);
            assert_eq!(visited.len(), nodes.len());
            released
        };
        assert_eq!(
            release_checkpoint(checkpoints.remove(0)),
            [Released::Environment(1)]
        );
        assert_eq!(
            release_checkpoint(checkpoints.remove(0)),
            [
                Released::Loan(formed_instances[0].0, 0),
                Released::Environment(formed_instances[0].0),
                Released::Environment(1),
                Released::Environment(formed_instances[0].1),
            ]
        );
        assert_eq!(
            release_checkpoint(checkpoints.remove(0)),
            [
                Released::Loan(formed_instances[1].0, 1),
                Released::Environment(formed_instances[1].0),
                Released::Loan(formed_instances[0].0, 0),
                Released::Environment(formed_instances[0].0),
                Released::Environment(1),
                Released::Environment(formed_instances[0].1),
                Released::Environment(formed_instances[1].1),
            ]
        );
    }

    #[test]
    fn independent_closure_survives_a_recursive_loop_into_the_next_loop() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-then-independent.ko",
                "fun run(flags: List<Int>, next: List<Int>) {
var f: move () -> Unit = move {}
val leaf: move () -> Unit = move {}
var g: move () -> Unit = move { leaf() }
for (_ in flags) { f = move { f() } }
val first = f()
for (_ in next) {}
val second = g() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some());
        let statements = checker
            .iterations
            .values()
            .map(|iteration| iteration.descriptor().statement())
            .collect::<Vec<_>>();
        assert_eq!(statements.len(), 2);
        let (first, second) = (statements[0], statements[1]);
        let second_graph = &planner.loop_capture_graphs[&second.index()];
        let g = planner.loop_phis[&second.index()]
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Header
                    && phi.root_nodes().iter().any(|&node| {
                        sources.slice(
                            parsed
                                .ast()
                                .expressions()
                                .get(second_graph.nodes()[node].closure())
                                .unwrap()
                                .span(),
                        ) == Ok("move { leaf() }")
                    })
            })
            .unwrap();
        assert!(!planner.recursive_phi_bindings.contains(&g.owner()));
        assert!(!g.origins().is_empty());
        let first_exit = planner.loop_phis[&first.index()]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == g.symbol())
            .unwrap();
        assert!(!first_exit.origins().is_empty());
        let entry = planner.loop_phi_incomings[&second.index()]
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap()
            .bindings()
            .iter()
            .find(|binding| binding.target() == g.owner())
            .unwrap();
        assert_eq!(entry.values().len(), 1);
        assert_eq!(entry.root_sources().len(), 1);
        assert_eq!(entry.root_sources()[0].source(), entry.values()[0].source());
        assert!(!entry.origins().is_empty());
        assert!(entry.selector_writes().iter().any(|write| {
            write.node() != g.root_nodes()[0] && write.condition() != CleanupConditionId::NEVER
        }));
    }

    #[test]
    fn snapshot_keeps_distinct_same_lambda_phi_and_ordinary_roots() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-overlapping-roots.ko",
                "fun run(first: List<Boolean>, second: List<Int>, pick: Boolean) {
var f: move () -> Unit = move {}
var g: move () -> Unit = move {}
for (recurse in first) {
    val next: move () -> Unit = move {}
    f = move { f() }
    if (recurse) { g = next } else { f = next }
}
var h: move () -> Unit = if (pick) f else g
for (_ in second) { h = move { h() } }
val used = h() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        let candidate = planner.into_candidate_facts();
        assert_eq!(candidate.iterations.len(), 2);
        let plans = candidate
            .iterations
            .iter()
            .map(|plan| (plan.descriptor().statement().index(), plan))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(plans.len(), candidate.iterations.len());
        let table = &candidate.cleanup_conditions;
        let steps = &candidate.cleanup_steps;
        let statements = checker
            .iterations
            .values()
            .map(|plan| plan.descriptor().statement())
            .collect::<Vec<_>>();
        assert_eq!(statements.len(), 2);
        let f = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("f"))
            .unwrap()
            .id();
        let first_exit = plans[&statements[0].index()]
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == f)
            .unwrap();
        let g = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("g"))
            .unwrap()
            .id();
        let g_exit = plans[&statements[0].index()]
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == g)
            .unwrap();
        let h = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("h"))
            .unwrap()
            .id();
        let second_header = plans[&statements[1].index()]
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == h)
            .unwrap();
        let entry = plans[&statements[1].index()]
            .closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap()
            .bindings()
            .iter()
            .find(|binding| binding.target() == second_header.owner())
            .unwrap();
        let snapshot = entry.values()[0].source();
        let graph = plans[&statements[1].index()].capture_graph();
        let same_lambda = first_exit
            .root_origins()
            .find(|root| {
                g_exit
                    .root_origins()
                    .any(|other| other.closure() == root.closure())
            })
            .unwrap();
        let node = *second_header
            .root_nodes()
            .iter()
            .find(|&&node| graph.nodes()[node].closure() == same_lambda.closure())
            .unwrap();
        let same_node_sources = entry
            .root_sources()
            .iter()
            .filter(|source| source.node() == node && source.source() == snapshot)
            .collect::<Vec<_>>();
        assert_eq!(same_node_sources.len(), 2);
        assert_eq!(
            (*table).clone().and(
                same_node_sources[0].condition(),
                same_node_sources[1].condition(),
            ),
            CleanupConditionId::NEVER
        );
        let saved = table.owner_snapshot(snapshot).unwrap();
        assert_eq!(saved.value_inputs().len(), 2);
        assert!(
            saved
                .value_inputs()
                .iter()
                .any(|input| input.owner() == first_exit.owner())
        );
        assert!(
            saved
                .value_inputs()
                .iter()
                .any(|input| input.owner() == g_exit.owner())
        );
        let snapshot_point = DropPoint::AfterExpression(saved.value());
        let save_at = unique_action_index(steps, snapshot_point, |action| {
            matches!(action, IterationCleanupAction::SaveOwnerSnapshot { condition: None, owner, value }
                if *owner == snapshot && *value == saved.value())
        });
        let commit_at = unique_action_index(steps, snapshot_point, |action| {
            matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                if *owner == snapshot && *target == h)
        });
        assert!(save_at < commit_at);
        let pick_selector = saved
            .copies()
            .iter()
            .map(|copy| copy.source())
            .find(|&selector| {
                table.selector(selector).is_some_and(|selector| {
                    sources
                        .slice(selector.origin())
                        .is_ok_and(|text| text.contains("pick"))
                })
            })
            .unwrap();
        let f_root = same_lambda;
        let g_root = g_exit
            .root_origins()
            .find(|root| root.closure() == same_lambda.closure())
            .unwrap();
        let edge = plans[&statements[1].index()]
            .closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
        let first_phis = plans[&statements[0].index()].closure_phis();
        let header = |symbol| {
            first_phis
                .iter()
                .find(|phi| {
                    phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == symbol
                })
                .unwrap()
        };
        let (f_header, g_header) = (header(f), header(g));
        let first_edges = plans[&statements[0].index()].closure_phi_incomings();
        let first_edge = |kind| first_edges.iter().find(|edge| edge.kind() == kind).unwrap();
        fn binding(
            edge: &crate::ownership_checking::IterationPhiIncoming,
            owner: CleanupOwnerValueId,
        ) -> &IterationPhiIncomingBinding {
            edge.bindings()
                .iter()
                .find(|binding| binding.target() == owner)
                .unwrap()
        }
        let first_entry = first_edge(IterationPhiIncomingKind::Entry);
        let first_backedge = first_edge(IterationPhiIncomingKind::Fallthrough);
        let first_exhausted = first_edge(IterationPhiIncomingKind::Exhaustion);
        let initial = [f_header.owner(), g_header.owner()]
            .map(|owner| binding(first_entry, owner).values()[0].source());
        let f_back = binding(first_backedge, f_header.owner());
        let g_back = binding(first_backedge, g_header.owner());
        fn unique_action_index(
            actions: &[(DropPoint, IterationCleanupAction)],
            at: DropPoint,
            matches: impl Fn(&IterationCleanupAction) -> bool,
        ) -> usize {
            let found = actions
                .iter()
                .enumerate()
                .filter_map(|(index, (point, action))| matches(action).then_some((index, *point)))
                .collect::<Vec<_>>();
            let [(index, point)] = found.as_slice() else {
                panic!("one matching action must exist for {at:?}")
            };
            assert_eq!(*point, at);
            *index
        }
        fn assert_no_root_cleanup_at_edge(
            steps: &[(DropPoint, IterationCleanupAction)],
            table: &CleanupConditions,
            point: DropPoint,
            choices: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
        ) {
            let unexpected = steps
                .iter()
                .filter_map(|(at, action)| {
                    let root = match action {
                        IterationCleanupAction::Drop(root)
                        | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                        _ => return None,
                    };
                    (*at == point
                        && root
                            .condition()
                            .is_none_or(|condition| selected(table, condition, choices)))
                    .then_some(*action)
                })
                .collect::<Vec<_>>();
            assert!(
                unexpected.is_empty(),
                "root cleanup during edge transport at {point:?}: {unexpected:?}"
            );
        }
        let created_closure = |wanted| {
            let matches = steps
                .iter()
                .filter_map(|(_, action)| match action {
                    IterationCleanupAction::CreateClosureOwner { owner, closure }
                        if *owner == wanted =>
                    {
                        Some(*closure)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [closure] = matches.as_slice() else {
                return None;
            };
            Some(*closure)
        };
        let recursive_snapshot = f_back
            .values()
            .iter()
            .map(|value| value.source())
            .find(|&owner| {
                let source = table.owner_snapshot(owner).unwrap().value_inputs()[0].owner();
                created_closure(source).is_some_and(|closure| {
                    sources.slice(parsed.ast().expressions().get(closure).unwrap().span())
                        == Ok("move { f() }")
                })
            })
            .unwrap();
        let f_branch_snapshot = f_back
            .values()
            .iter()
            .map(|value| value.source())
            .find(|&owner| owner != recursive_snapshot)
            .unwrap();
        let g_branch_snapshot = g_back
            .values()
            .iter()
            .map(|value| value.source())
            .find(|&owner| owner != g_header.owner())
            .unwrap();
        let next_snapshot = table
            .owner_snapshot(f_branch_snapshot)
            .unwrap()
            .value_inputs()[0]
            .owner();
        assert_eq!(
            table
                .owner_snapshot(g_branch_snapshot)
                .unwrap()
                .value_inputs()[0]
                .owner(),
            next_snapshot
        );
        let next_owner = table.owner_snapshot(next_snapshot).unwrap().value_inputs()[0].owner();
        let recursive_owner = table
            .owner_snapshot(recursive_snapshot)
            .unwrap()
            .value_inputs()[0]
            .owner();
        for owner in initial {
            let closure = created_closure(owner).unwrap();
            unique_action_index(steps, DropPoint::AfterExpression(closure), |action| {
                matches!(action, IterationCleanupAction::CreateClosureOwner { owner: created, closure: formed }
                    if *created == owner && *formed == closure)
            });
        }
        let next_closure = created_closure(next_owner).unwrap();
        let next_point = DropPoint::AfterExpression(next_closure);
        let next_create = unique_action_index(steps, next_point, |action| {
            matches!(action, IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *owner == next_owner && *closure == next_closure)
        });
        let next_save = unique_action_index(steps, next_point, |action| {
            matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                if *owner == next_snapshot && *value == next_closure)
        });
        let next_commit = unique_action_index(steps, next_point, |action| {
            matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                if *owner == next_snapshot && sources.slice(names.symbols()[target.index()].span()) == Ok("next"))
        });
        assert!(next_create < next_save && next_save < next_commit);
        let recursive_closure = created_closure(recursive_owner).unwrap();
        let recursive_point = DropPoint::AfterExpression(recursive_closure);
        let recursive_create = unique_action_index(steps, recursive_point, |action| {
            matches!(action, IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *owner == recursive_owner && *closure == recursive_closure)
        });
        let recursive_capture = unique_action_index(steps, recursive_point, |action| {
            matches!(action, IterationCleanupAction::SaveClosureCapture { owner, .. }
                if *owner == recursive_owner)
        });
        let recursive_save = unique_action_index(steps, recursive_point, |action| {
            matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                if *owner == recursive_snapshot && *value == recursive_closure)
        });
        let recursive_commit = unique_action_index(steps, recursive_point, |action| {
            matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                if *owner == recursive_snapshot && *target == f)
        });
        assert!(
            recursive_create < recursive_capture
                && recursive_capture < recursive_save
                && recursive_save < recursive_commit
        );
        let IterationCleanupAction::SaveClosureCapture {
            target: slot,
            input,
            ..
        } = steps[recursive_capture].1
        else {
            unreachable!()
        };
        let first_graph = plans[&statements[0].index()].capture_graph();
        let first_nodes = |instances: &BTreeMap<usize, usize>| {
            instances
                .iter()
                .map(|(&instance, &node)| {
                    let closure = graph.nodes()[node].closure();
                    let first_node = first_graph
                        .nodes()
                        .iter()
                        .position(|candidate| candidate.closure() == closure)
                        .unwrap();
                    (instance, first_node)
                })
                .collect::<BTreeMap<_, _>>()
        };
        let mut next_instance = 0;
        let mut handles = BTreeMap::new();
        let mut instances = BTreeMap::new();
        let mut form = |owner, handles: &mut BTreeMap<_, _>, instances: &mut BTreeMap<_, _>| {
            let closure = created_closure(owner).unwrap();
            next_instance += 1;
            let node = graph
                .nodes()
                .iter()
                .position(|candidate| candidate.closure() == closure)
                .unwrap();
            assert!(instances.insert(next_instance, node).is_none());
            assert!(handles.insert(owner, next_instance).is_none());
            next_instance
        };
        let mut choices = table
            .nodes()
            .iter()
            .filter_map(|node| match node {
                CleanupCondition::Choice { selector, .. } => Some((*selector, 0)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        for owner in initial {
            form(owner, &mut handles, &mut instances);
        }
        let mut captured = BTreeMap::new();
        assert_no_root_cleanup_at_edge(steps, table, first_entry.point(), &choices);
        let entry_values = replay_captured_edge(
            table,
            first_graph,
            first_phis,
            first_entry,
            &handles,
            &first_nodes(&instances),
            &captured,
            &mut choices,
        );
        assert_eq!(entry_values.len(), 2);
        for (target, (source, instance)) in entry_values {
            assert_eq!(handles.remove(&source), Some(instance));
            assert!(handles.insert(target, instance).is_none());
        }
        let branch_condition =
            |owner| table.owner_snapshot(owner).unwrap().value_inputs()[0].condition();
        let selected_loop_actions = |at, aliases: &BTreeSet<_>, choices: &BTreeMap<_, _>| {
            steps
                .iter()
                .filter_map(|(point, action)| {
                    let root = match action {
                        IterationCleanupAction::Drop(root)
                        | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                        _ => return None,
                    };
                    (*point == at
                        && (matches!(root.target(), DropTarget::Named(symbol) if symbol == f || symbol == g)
                            || root.owner().is_some_and(|owner| aliases.contains(&owner))
                            || root
                                .instance_address()
                                .and_then(|address| table.instance_address(address))
                                .is_some_and(|address| aliases.contains(&address.root())))
                        && root
                            .condition()
                            .is_none_or(|condition| selected(table, condition, choices)))
                    .then_some((*point, *action))
                })
                .collect::<Vec<_>>()
        };
        let mut released = BTreeSet::new();
        // 首轮 g 接收 next，次轮 f 接收同一 lambda 的新实例；递归旧 f 链在次轮替换时释放。
        for recurse in [0, 1] {
            let next_instance = form(next_owner, &mut handles, &mut instances);
            assert_eq!(handles.remove(&next_owner), Some(next_instance));
            assert!(handles.insert(next_snapshot, next_instance).is_none());
            replay_snapshot_choices(table, next_snapshot, &mut choices);

            let recursive_instance = form(recursive_owner, &mut handles, &mut instances);
            assert_eq!(input.value(), CleanupCaptureValue::Owner(f_header.owner()));
            assert_eq!(
                (input.mode(), input.effect()),
                (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
            );
            assert!(selected(table, input.condition(), &choices));
            let position = table.capture_slot_value(slot).unwrap().position();
            let previous = handles.remove(&f_header.owner()).unwrap();
            let nodes = first_nodes(&instances);
            assert!(
                first_graph.nodes()[nodes[&recursive_instance]]
                    .sources()
                    .iter()
                    .any(|source| source.position() == position
                        && source.captured().contains(&nodes[&previous]))
            );
            assert!(
                captured
                    .insert((recursive_instance, position), previous)
                    .is_none()
            );
            assert_eq!(handles.remove(&recursive_owner), Some(recursive_instance));
            assert!(
                handles
                    .insert(recursive_snapshot, recursive_instance)
                    .is_none()
            );
            replay_snapshot_choices(table, recursive_snapshot, &mut choices);

            let recurse_selectors = table
                .nodes()
                .iter()
                .filter_map(|node| match node {
                    CleanupCondition::Choice { selector, .. }
                        if table.selector(*selector).is_some_and(|selector| {
                            sources
                                .slice(selector.origin())
                                .is_ok_and(|text| text.contains("recurse"))
                        }) =>
                    {
                        Some(*selector)
                    }
                    _ => None,
                })
                .collect::<BTreeSet<_>>();
            let matching = recurse_selectors
                .into_iter()
                .filter(|selector| {
                    let mut yes = choices.clone();
                    yes.insert(*selector, 0);
                    let mut no = choices.clone();
                    no.insert(*selector, 1);
                    selected(table, branch_condition(g_branch_snapshot), &yes)
                        && !selected(table, branch_condition(g_branch_snapshot), &no)
                        && !selected(table, branch_condition(f_branch_snapshot), &yes)
                        && selected(table, branch_condition(f_branch_snapshot), &no)
                })
                .collect::<Vec<_>>();
            let [recurse_selector] = matching.as_slice() else {
                panic!("one evaluated recurse selector must choose the assignment")
            };
            choices.insert(*recurse_selector, recurse);
            let branch_snapshot = if recurse == 0 {
                g_branch_snapshot
            } else {
                f_branch_snapshot
            };
            assert!(selected(table, branch_condition(branch_snapshot), &choices));
            assert_eq!(
                table
                    .owner_snapshot(branch_snapshot)
                    .unwrap()
                    .value_inputs()[0]
                    .owner(),
                next_snapshot
            );
            let next_instance = handles.remove(&next_snapshot).unwrap();
            assert!(handles.insert(branch_snapshot, next_instance).is_none());
            replay_snapshot_choices(table, branch_snapshot, &mut choices);
            let replaced_symbol = if recurse == 0 { g } else { f };
            let point =
                DropPoint::AfterExpression(table.owner_snapshot(branch_snapshot).unwrap().value());
            let aliases = BTreeSet::from([
                f_header.owner(),
                g_header.owner(),
                recursive_owner,
                recursive_snapshot,
                next_snapshot,
                branch_snapshot,
            ]);
            let actions = selected_loop_actions(point, &aliases, &choices);
            let [(actual_point, action)] = actions.as_slice() else {
                panic!("one replaced binding must be cleaned at its assignment")
            };
            assert_eq!(*actual_point, point);
            let branch_save = unique_action_index(steps, point, |action| {
                matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner, .. }
                    if *owner == branch_snapshot)
            });
            let branch_cleanup = unique_action_index(steps, point, |candidate| candidate == action);
            let branch_commit = unique_action_index(steps, point, |action| {
                matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *owner == branch_snapshot && *target == replaced_symbol)
            });
            assert!(branch_save < branch_cleanup && branch_cleanup < branch_commit);
            match (recurse, action) {
                (0, IterationCleanupAction::Drop(root)) => {
                    assert_eq!(root.owner(), Some(g_header.owner()));
                }
                (1, IterationCleanupAction::ReleaseClosureInstances { layout, root }) => {
                    assert_eq!(*layout, ClosureReleaseLayout::Iteration(statements[0]));
                    assert_eq!(root.owner(), Some(recursive_owner));
                }
                other => panic!("unexpected replacement cleanup: {other:?}"),
            }
            if recurse == 0 {
                assert!(released.insert(handles.remove(&g_header.owner()).unwrap()));
            } else {
                let root = handles.remove(&recursive_snapshot).unwrap();
                for instance in replay_owned_closure_release(
                    first_graph,
                    &first_nodes(&instances),
                    &mut captured,
                    root,
                ) {
                    assert!(released.insert(instance));
                }
            }
            assert_no_root_cleanup_at_edge(steps, table, first_backedge.point(), &choices);
            let transported = replay_captured_edge(
                table,
                first_graph,
                first_phis,
                first_backedge,
                &handles,
                &first_nodes(&instances),
                &captured,
                &mut choices,
            );
            assert_eq!(transported.len(), 2);
            for (target, (source, instance)) in transported {
                assert_eq!(handles.remove(&source), Some(instance));
                assert!(handles.insert(target, instance).is_none());
            }
        }
        assert_eq!(released.len(), 4);
        assert!(captured.is_empty());
        assert_no_root_cleanup_at_edge(steps, table, first_exhausted.point(), &choices);
        let exit_values = replay_captured_edge(
            table,
            first_graph,
            first_phis,
            first_exhausted,
            &handles,
            &first_nodes(&instances),
            &captured,
            &mut choices,
        );
        assert_eq!(exit_values.len(), 2);
        for (target, (source, instance)) in exit_values {
            assert_eq!(handles.remove(&source), Some(instance));
            assert!(handles.insert(target, instance).is_none());
        }
        let exit_roots = BTreeSet::from([first_exit.owner(), g_exit.owner()]);
        assert!(
            selected_loop_actions(
                DropPoint::AfterStatement(statements[0]),
                &exit_roots,
                &choices,
            )
            .is_empty(),
            "both exit roots must survive until the pick snapshot"
        );
        assert_ne!(handles[&first_exit.owner()], handles[&g_exit.owner()]);
        assert_eq!(instances[&handles[&first_exit.owner()]], node);
        assert_eq!(instances[&handles[&g_exit.owner()]], node);
        assert_eq!(choices[&first_exit.availability_selector()], 1);
        assert_eq!(choices[&g_exit.availability_selector()], 1);
        assert_eq!(choices[&f_root.selector()], 1);
        assert_eq!(choices[&g_root.selector()], 1);
        let mut selected_instances = Vec::new();
        let mut selected_sources = Vec::new();
        for (pick, expects_f) in [(0, true), (1, false)] {
            let mut choices = choices.clone();
            let mut handles = handles.clone();
            let mut captured = captured.clone();
            let mut released = released.clone();
            choices.insert(pick_selector, pick);
            assert!(
                selected_loop_actions(snapshot_point, &exit_roots, &choices).is_empty(),
                "the pick snapshot must not clean either exit root"
            );
            let selected_values = saved
                .value_inputs()
                .iter()
                .filter(|input| selected(table, input.condition(), &choices))
                .collect::<Vec<_>>();
            let [selected_value] = selected_values.as_slice() else {
                panic!("the snapshot must read exactly one old root handle")
            };
            assert_eq!(
                selected_value.owner(),
                if expects_f {
                    first_exit.owner()
                } else {
                    g_exit.owner()
                }
            );
            let chosen = handles.remove(&selected_value.owner()).unwrap();
            let unselected_owner = if expects_f {
                g_exit.owner()
            } else {
                first_exit.owner()
            };
            let unselected = handles.remove(&unselected_owner).unwrap();
            let point = DropPoint::BranchExit {
                control: saved.value(),
                branch: pick,
            };
            let root_owners = BTreeSet::from([first_exit.owner(), g_exit.owner()]);
            let actions = steps
                .iter()
                .filter_map(|(at, action)| {
                    let root = match action {
                        IterationCleanupAction::Drop(root)
                        | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                        _ => return None,
                    };
                    (*at == point
                        && (matches!(root.target(), DropTarget::Named(symbol) if symbol == f || symbol == g)
                            || root.owner().is_some_and(|owner| root_owners.contains(&owner))
                            || root
                                .instance_address()
                                .and_then(|address| table.instance_address(address))
                                .is_some_and(|address| root_owners.contains(&address.root())))
                        && root.condition().is_none_or(|condition| selected(table, condition, &choices)))
                    .then_some((*at, *action))
                })
                .collect::<Vec<_>>();
            let [(_, action)] = actions.as_slice() else {
                panic!("only the unselected root may have branch cleanup")
            };
            let branch_release_at =
                unique_action_index(steps, point, |candidate| candidate == action);
            assert!(branch_release_at < save_at && save_at < commit_at);
            match (pick, action) {
                (0, IterationCleanupAction::Drop(root)) => {
                    assert_eq!(root.owner(), Some(g_exit.owner()));
                }
                (1, IterationCleanupAction::ReleaseClosureInstances { layout, root }) => {
                    assert_eq!(*layout, ClosureReleaseLayout::Iteration(statements[0]));
                    assert_eq!(root.owner(), Some(first_exit.owner()));
                }
                other => panic!("unexpected snapshot cleanup: {other:?}"),
            }
            assert_ne!(chosen, unselected);
            for instance in
                replay_owned_closure_release(graph, &instances, &mut captured, unselected)
            {
                assert!(released.insert(instance));
            }
            replay_snapshot_choices(table, snapshot, &mut choices);
            let active = same_node_sources
                .iter()
                .filter(|source| selected(table, source.condition(), &choices))
                .collect::<Vec<_>>();
            assert_eq!(active.len(), 1);
            selected_sources.push(active[0].condition());
            assert!(handles.insert(snapshot, chosen).is_none());
            assert_no_root_cleanup_at_edge(steps, table, edge.point(), &choices);
            let transported = replay_captured_edge(
                table,
                graph,
                plans[&statements[1].index()].closure_phis(),
                edge,
                &handles,
                &instances,
                &captured,
                &mut choices,
            );
            assert_eq!(transported[&second_header.owner()], (snapshot, chosen));
            assert_eq!(handles.remove(&snapshot), Some(chosen));
            assert!(handles.insert(second_header.owner(), chosen).is_none());
            let selector = second_header
                .root_origins()
                .find(|root| root.node() == node)
                .unwrap()
                .selector();
            assert_eq!(choices[&selector], 1);
            selected_instances.push(chosen);
            let exhausted = plans[&statements[1].index()]
                .closure_phi_incomings()
                .iter()
                .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
                .unwrap();
            let h_exit = plans[&statements[1].index()]
                .closure_phis()
                .iter()
                .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == h)
                .unwrap();
            assert_no_root_cleanup_at_edge(steps, table, exhausted.point(), &choices);
            let transported = replay_captured_edge(
                table,
                graph,
                plans[&statements[1].index()].closure_phis(),
                exhausted,
                &handles,
                &instances,
                &captured,
                &mut choices,
            );
            assert_eq!(
                transported[&h_exit.owner()],
                (second_header.owner(), chosen)
            );
            assert_eq!(handles.remove(&second_header.owner()), Some(chosen));
            assert!(handles.insert(h_exit.owner(), chosen).is_none());
            let aliases = BTreeSet::from([snapshot, second_header.owner(), h_exit.owner()]);
            let final_actions = steps
                .iter()
                .filter_map(|(point, action)| {
                    let root = match action {
                        IterationCleanupAction::Drop(root)
                        | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                        _ => return None,
                    };
                    (root.target() == DropTarget::Named(h)
                        || root.owner().is_some_and(|owner| aliases.contains(&owner))
                        || root
                            .instance_address()
                            .and_then(|address| table.instance_address(address))
                            .is_some_and(|address| aliases.contains(&address.root())))
                    .then_some((point, action, root))
                })
                .filter(|(_, _, root)| {
                    root.condition()
                        .is_none_or(|condition| selected(table, condition, &choices))
                })
                .map(|(point, action, _)| (*point, *action))
                .collect::<Vec<_>>();
            let final_call = parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, expression)| sources.slice(expression.span()) == Ok("h()"))
                .max_by_key(|(_, expression)| expression.span().start())
                .unwrap()
                .0;
            let [(point, IterationCleanupAction::ReleaseClosureInstances { layout, root })] =
                final_actions.as_slice()
            else {
                panic!("the selected h instance must release once at its call return")
            };
            assert_eq!(*point, DropPoint::CallReturn(final_call));
            assert_eq!(*layout, ClosureReleaseLayout::Iteration(statements[1]));
            assert_eq!(root.owner(), Some(h_exit.owner()));
            for instance in replay_owned_closure_release(
                graph,
                &instances,
                &mut captured,
                handles.remove(&h_exit.owner()).unwrap(),
            ) {
                assert!(released.insert(instance));
            }
            assert_eq!(released.len(), instances.len());
            assert!(handles.is_empty() && captured.is_empty());
        }
        assert_ne!(selected_instances[0], selected_instances[1]);
        assert_ne!(selected_sources[0], selected_sources[1]);
    }

    fn assert_cross_loop_parent_release(
        source_text: &str,
        outer_text: &str,
        capture_count: usize,
        binding_name: &str,
        from_environment: bool,
        needs_file_release: bool,
        expect_source_guard_copy: bool,
    ) {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("cross-loop-release-root.ko", source_text)
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let checked =
            crate::ownership_checking::check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert!(checked.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some());
        let candidate = planner.into_candidate_facts();
        let plans = candidate
            .iterations
            .iter()
            .map(|plan| (plan.descriptor().statement().index(), plan))
            .collect::<BTreeMap<_, _>>();
        let steps = &candidate.cleanup_steps;
        let table = &candidate.cleanup_conditions;
        let (outer, closure) = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if sources.slice(parsed.ast().expressions().get(*closure).unwrap().span())
                        == Ok(outer_text) =>
                {
                    Some((*owner, *closure))
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(checked.captures_of(closure).count(), capture_count);
        assert_eq!(
            table
                .closure_capture_edges(outer)
                .unwrap()
                .iter()
                .any(|edge| {
                    matches!(
                        edge.input().value(),
                        CleanupCaptureValue::Environment { .. }
                    )
                }),
            from_environment
        );
        let outer_symbol = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok(binding_name))
            .unwrap()
            .id();
        let outer_actions = steps
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances { layout, root }
                    if root.target() == DropTarget::Named(outer_symbol) =>
                {
                    Some((*layout, *root))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let ordinary_drops = steps
            .iter()
            .filter(|(_, action)| {
                matches!(action, IterationCleanupAction::Drop(fact)
                if fact.target() == DropTarget::Named(outer_symbol))
            })
            .count();
        if needs_file_release {
            let [(layout, root)] = outer_actions.as_slice() else {
                panic!("parent needs exactly one release action: {outer_actions:?}")
            };
            assert_eq!(*layout, ClosureReleaseLayout::File);
            assert_eq!(root.owner(), Some(outer));
            let DropPoint::CallReturn(call) = root.point() else {
                panic!("the File root must release after its call")
            };
            let expected_call = format!("{binding_name}()");
            assert_eq!(
                sources.slice(parsed.ast().expressions().get(call).unwrap().span()),
                Ok(expected_call.as_str())
            );
            assert_eq!(ordinary_drops, 0);
            let all_outer_roots = steps
                .iter()
                .filter_map(|(_, action)| match action {
                    IterationCleanupAction::Drop(fact)
                    | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. }
                        if fact.owner() == Some(outer) =>
                    {
                        Some(*action)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                all_outer_roots,
                [IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::File,
                    root: *root,
                }]
            );
        } else {
            assert!(outer_actions.is_empty());
            assert_eq!(ordinary_drops, 1);
        }
        let borrowed_child = if capture_count == 3 {
            let borrowed = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, expression)| {
                    (sources.slice(expression.span()) == Ok("{ read(xs) }")).then_some(id)
                })
                .unwrap();
            let borrowed_captures = checked.captures_of(borrowed).collect::<Vec<_>>();
            let [loan] = borrowed_captures.as_slice() else {
                panic!("the borrowed child must have one shared source")
            };
            let ClosureCaptureSource::Symbol(source_symbol) = loan.source() else {
                panic!("the shared source must be the owned local list")
            };
            assert_eq!(
                (loan.mode(), loan.effect()),
                (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow)
            );
            let release_index = steps
                .iter()
                .position(|(_, action)| {
                    matches!(action,
                    IterationCleanupAction::ReleaseClosureInstances {
                        layout: ClosureReleaseLayout::File,
                        root,
                    } if root.owner() == Some(outer))
                })
                .unwrap();
            let source_expression = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, expression)| {
                    (sources.slice(expression.span()) == Ok("listOf(1)")).then_some(id)
                })
                .unwrap();
            let source_drops = steps
                .iter()
                .enumerate()
                .filter_map(|(index, (_, action))| match action {
                    IterationCleanupAction::Drop(fact)
                        if fact.target() == DropTarget::Named(source_symbol)
                            || fact.target() == DropTarget::Temporary(source_expression)
                            || fact.target() == DropTarget::RetainedSource(loan.source())
                            || matches!(fact.target(), DropTarget::Captured { source, .. }
                                if source == loan.source()) =>
                    {
                        Some((index, *fact))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(source_drop_index, source_drop)] = source_drops.as_slice() else {
                panic!("the shared source must have one drop: {source_drops:?}")
            };
            assert_eq!(source_drop.target(), DropTarget::Named(source_symbol));
            assert!(
                *source_drop_index > release_index,
                "the shared source must drop after the File root: {source_drops:?}"
            );
            assert_eq!(
                source_drop.point(),
                steps[release_index].0,
                "the named source must drop immediately after the root ends its final loan"
            );
            assert!(matches!(source_drop.point(), DropPoint::CallReturn(_)));
            let borrowed_symbol = names
                .symbols()
                .iter()
                .find(|symbol| sources.slice(symbol.span()) == Ok("g"))
                .unwrap()
                .id();
            let (position, capture) = checked
                .captures_of(closure)
                .enumerate()
                .find(|(_, capture)| {
                    capture.source() == ClosureCaptureSource::Symbol(borrowed_symbol)
                })
                .unwrap();
            assert_eq!(position, 2);
            assert_eq!(
                (capture.mode(), capture.effect()),
                (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
            );
            let parent_input = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if *owner == outer && input.source() == capture.source() => {
                        assert_eq!(
                            table.capture_slot_value(*target).unwrap().position(),
                            position
                        );
                        Some(*input)
                    }
                    _ => None,
                })
                .unwrap();
            let child_owner = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CreateClosureOwner { owner, closure }
                        if *closure == borrowed =>
                    {
                        Some(*owner)
                    }
                    _ => None,
                })
                .unwrap();
            let child_snapshot = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                        if *value == borrowed =>
                    {
                        Some(*owner)
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                parent_input.value(),
                CleanupCaptureValue::Owner(child_snapshot)
            );
            let child_inputs = steps
                .iter()
                .filter_map(|(_, action)| match action {
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if *owner == child_owner => Some((*target, *input)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(target, child_input)] = child_inputs.as_slice() else {
                panic!("the borrowed child must form exactly one shared capture")
            };
            let child_slot = table.capture_slot_value(*target).unwrap();
            assert_eq!(child_slot.environment(), child_owner);
            assert_eq!(child_slot.position(), 0);
            assert_eq!(child_slot.source(), loan.source());
            assert_eq!(child_input.source(), loan.source());
            assert_eq!(
                (child_input.mode(), child_input.effect()),
                (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow)
            );
            assert!(!steps.iter().any(|(_, action)| matches!(action,
                IterationCleanupAction::EndCaptureLoan { closure, .. }
                    if *closure == borrowed)));
            Some((
                borrowed_symbol,
                borrowed,
                child_owner,
                child_snapshot,
                *child_input,
                child_slot.position(),
                *source_drop,
            ))
        } else {
            None
        };
        if capture_count != 2 && capture_count != 3 {
            assert!(!expect_source_guard_copy);
            return;
        }

        // Execute one round in each recursive loop before forming the outside parent.
        let statements = checker
            .iterations
            .values()
            .map(|plan| plan.descriptor().statement())
            .collect::<Vec<_>>();
        assert_eq!(statements.len(), 2);
        let parent_saves = steps
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == outer => Some((*target, *input)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(parent_saves.len(), capture_count);
        for (position, ((target, input), capture)) in parent_saves
            .iter()
            .zip(checked.captures_of(closure))
            .enumerate()
        {
            let slot = table.capture_slot_value(*target).unwrap();
            assert_eq!(slot.environment(), outer);
            assert_eq!(slot.position(), position);
            assert_eq!(slot.source(), capture.source());
            assert_eq!(input.source(), capture.source());
            assert_eq!(
                (input.mode(), input.effect()),
                (capture.mode(), capture.effect())
            );
        }
        let mut next_instance = 0;
        let mut instances = BTreeMap::new();
        let mut owners = BTreeMap::new();
        let mut source_instances = BTreeMap::new();
        let mut captures = BTreeMap::new();
        let mut shared_loans = BTreeMap::new();
        let mut choices = BTreeMap::new();
        let mut chains = Vec::new();
        let mut carried_roots = Vec::new();
        let mut exit_roots = Vec::new();
        for statement in statements {
            let phis = &plans[&statement.index()].closure_phis();
            let graph = &plans[&statement.index()].capture_graph();
            let (parent_slot, symbol) = parent_saves
                .iter()
                .find_map(|(target, input)| {
                    let ClosureCaptureSource::Symbol(symbol) = input.source() else {
                        return None;
                    };
                    let header = phis.iter().find(|phi| {
                        phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == symbol
                    })?;
                    steps
                        .iter()
                        .any(|(_, action)| {
                            matches!(action,
                            IterationCleanupAction::SaveClosureCapture { input, .. }
                                if input.value() == CleanupCaptureValue::Owner(header.owner()))
                        })
                        .then_some((target, symbol))
                })
                .unwrap();
            let phi = |boundary| {
                phis.iter()
                    .find(|phi| phi.boundary() == boundary && phi.symbol() == symbol)
                    .unwrap()
            };
            let (header, exit) = (
                phi(IterationPhiBoundary::Header),
                phi(IterationPhiBoundary::Exit),
            );
            let incomings = &plans[&statement.index()].closure_phi_incomings();
            let edge = |kind| incomings.iter().find(|edge| edge.kind() == kind).unwrap();
            let edge_values =
                |owners: &BTreeMap<CleanupOwnerValueId, usize>,
                 sources: &BTreeMap<CleanupOwnerValueId, usize>| {
                    let mut values = owners.clone();
                    for (&owner, &instance) in sources {
                        assert!(values.insert(owner, instance).is_none());
                    }
                    values
                };
            let forward_sources =
                |kind,
                 transported: &BTreeMap<CleanupOwnerValueId, (CleanupOwnerValueId, usize)>,
                 sources: &mut BTreeMap<CleanupOwnerValueId, usize>| {
                    for binding in edge(kind).bindings() {
                        if !binding.root_sources().is_empty() {
                            continue;
                        }
                        let &(source, instance) = &transported[&binding.target()];
                        if source == binding.target() {
                            assert_eq!(sources[&source], instance);
                        } else {
                            assert_eq!(sources.remove(&source), Some(instance));
                            assert!(sources.insert(binding.target(), instance).is_none());
                        }
                    }
                };
            let binding = |kind, target| {
                edge(kind)
                    .bindings()
                    .iter()
                    .find(|binding| binding.target() == target)
                    .unwrap()
            };
            let entry = binding(IterationPhiIncomingKind::Entry, header.owner());
            let [initial] = entry.root_sources() else {
                panic!("entry needs one formed root")
            };
            assert_eq!(entry.values()[0].source(), initial.source());
            let initial_snapshot = table.owner_snapshot(initial.source());
            let initial_source = if let Some(snapshot) = initial_snapshot {
                let [source] = snapshot.capture_inputs() else {
                    panic!("entry snapshot needs one formed owner")
                };
                assert!(selected(&table, source.condition(), &choices));
                source.owner()
            } else {
                initial.source()
            };
            let initial_create = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CreateClosureOwner { owner, closure }
                        if *owner == initial_source =>
                    {
                        Some(*closure)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| {
                    panic!("missing initial closure for {initial:?} in {statement:?}")
                });
            if initial_snapshot.is_some() {
                assert!(steps.iter().any(|(_, action)| matches!(action,
                    IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                        if *owner == initial.source() && *value == initial_create)));
                assert!(steps.iter().any(|(_, action)| matches!(action,
                    IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                        if *owner == initial.source() && *target == symbol)));
            }
            next_instance += 1;
            let seed = next_instance;
            assert!(instances.insert(seed, initial_create).is_none());
            assert!(owners.insert(initial_source, seed).is_none());
            let mut instance_nodes = instances
                .iter()
                .filter_map(|(&instance, &closure)| {
                    graph
                        .nodes()
                        .iter()
                        .position(|node| node.closure() == closure)
                        .map(|node| (instance, node))
                })
                .collect::<BTreeMap<_, _>>();
            if initial_snapshot.is_some() {
                let instance = owners.remove(&initial_source).unwrap();
                assert!(owners.insert(initial.source(), instance).is_none());
                replay_snapshot_choices(&table, initial.source(), &mut choices);
            }
            assert!(selected(&table, initial.condition(), &choices));
            for source_binding in edge(IterationPhiIncomingKind::Entry)
                .bindings()
                .iter()
                .filter(|binding| binding.root_sources().is_empty())
            {
                let [source] = source_binding.values() else {
                    panic!("ordinary source needs one entry value")
                };
                if !source_instances.contains_key(&source.source()) {
                    let Some(CleanupOwnerValue::Expression { expression, .. }) =
                        table.owner_value(source.source())
                    else {
                        panic!("ordinary source must originate from its evaluated expression")
                    };
                    assert_eq!(
                        sources.slice(parsed.ast().expressions().get(*expression).unwrap().span()),
                        Ok("listOf(1)")
                    );
                    next_instance += 1;
                    assert!(
                        source_instances
                            .insert(source.source(), next_instance)
                            .is_none()
                    );
                }
            }
            for (carried_symbol, carried_owner) in &carried_roots {
                let carry_header = phis
                    .iter()
                    .find(|phi| {
                        phi.boundary() == IterationPhiBoundary::Header
                            && phi.symbol() == *carried_symbol
                    })
                    .unwrap();
                let carry_entry = binding(IterationPhiIncomingKind::Entry, carry_header.owner());
                let [source] = carry_entry.values() else {
                    panic!("carried root needs one entry source")
                };
                assert_eq!(source.source(), *carried_owner);
                assert!(selected(&table, source.condition(), &choices));
            }
            let entry_values = replay_captured_edge(
                &table,
                graph,
                phis,
                edge(IterationPhiIncomingKind::Entry),
                &edge_values(&owners, &source_instances),
                &instance_nodes,
                &captures,
                &mut choices,
            );
            assert_eq!(
                entry_values.len(),
                edge(IterationPhiIncomingKind::Entry).bindings().len()
            );
            assert_eq!(entry_values[&header.owner()], (initial.source(), seed));
            forward_sources(
                IterationPhiIncomingKind::Entry,
                &entry_values,
                &mut source_instances,
            );
            for (carried_symbol, carried_owner) in &mut carried_roots {
                let carry_header = phis
                    .iter()
                    .find(|phi| {
                        phi.boundary() == IterationPhiBoundary::Header
                            && phi.symbol() == *carried_symbol
                    })
                    .unwrap();
                let instance = owners.remove(carried_owner).unwrap();
                assert_eq!(
                    entry_values[&carry_header.owner()],
                    (*carried_owner, instance)
                );
                assert!(owners.insert(carry_header.owner(), instance).is_none());
                *carried_owner = carry_header.owner();
            }
            assert_eq!(owners.remove(&initial.source()), Some(seed));
            assert!(owners.insert(header.owner(), seed).is_none());

            let (formed, recursive_closure, capture_slot, input) = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if input.value() == CleanupCaptureValue::Owner(header.owner()) => {
                        let closure = steps.iter().find_map(|(_, action)| match action {
                            IterationCleanupAction::CreateClosureOwner {
                                owner: created,
                                closure,
                            } if created == owner => Some(*closure),
                            _ => None,
                        })?;
                        Some((*owner, closure, *target, *input))
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(input.mode(), ClosureCaptureMode::Owned);
            assert_eq!(input.effect(), ClosureCaptureEffect::Move);
            assert!(selected(&table, input.condition(), &choices));
            next_instance += 1;
            let new_instance = next_instance;
            assert!(instances.insert(new_instance, recursive_closure).is_none());
            assert!(owners.insert(formed, new_instance).is_none());
            assert!(
                instance_nodes
                    .insert(
                        new_instance,
                        graph
                            .nodes()
                            .iter()
                            .position(|node| node.closure() == recursive_closure)
                            .unwrap(),
                    )
                    .is_none()
            );
            let position = table.capture_slot_value(capture_slot).unwrap().position();
            let prior = owners.remove(&header.owner()).unwrap();
            assert_eq!(prior, seed);
            assert!(captures.insert((new_instance, position), prior).is_none());
            let snapshot = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                        if *value == recursive_closure =>
                    {
                        Some(*owner)
                    }
                    _ => None,
                })
                .unwrap();
            assert!(steps.iter().any(|(_, action)| matches!(
                action,
                IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *owner == snapshot && *target == symbol
            )));
            let saved = table.owner_snapshot(snapshot).unwrap();
            let [source] = saved.capture_inputs() else {
                panic!("replacement must save the formed owner")
            };
            assert_eq!(source.owner(), formed);
            assert!(selected(&table, source.condition(), &choices));
            let instance = owners.remove(&formed).unwrap();
            assert!(owners.insert(snapshot, instance).is_none());
            replay_snapshot_choices(&table, snapshot, &mut choices);
            let backedge = binding(IterationPhiIncomingKind::Fallthrough, header.owner());
            let selected_values = backedge
                .values()
                .iter()
                .filter(|value| selected(&table, value.condition(), &choices))
                .collect::<Vec<_>>();
            let [back_value] = selected_values.as_slice() else {
                panic!("one backedge owner value must be selected")
            };
            let selected_roots = backedge
                .root_sources()
                .iter()
                .filter(|root| {
                    root.source() == back_value.source()
                        && root.node() == instance_nodes[&owners[&back_value.source()]]
                        && selected(&table, root.condition(), &choices)
                })
                .collect::<Vec<_>>();
            let [back_root] = selected_roots.as_slice() else {
                panic!("one backedge root must be selected")
            };
            assert_eq!(back_root.source(), snapshot);
            for (carried_symbol, carried_owner) in &carried_roots {
                let carry_backedge = binding(IterationPhiIncomingKind::Fallthrough, *carried_owner);
                let [source] = carry_backedge.values() else {
                    panic!("carried root needs one backedge source")
                };
                assert_eq!(source.source(), *carried_owner);
                assert!(selected(&table, source.condition(), &choices));
                assert!(phis.iter().any(|phi| {
                    phi.boundary() == IterationPhiBoundary::Header
                        && phi.symbol() == *carried_symbol
                        && phi.owner() == *carried_owner
                }));
            }
            let back_values = replay_captured_edge(
                &table,
                graph,
                phis,
                edge(IterationPhiIncomingKind::Fallthrough),
                &edge_values(&owners, &source_instances),
                &instance_nodes,
                &captures,
                &mut choices,
            );
            assert_eq!(
                back_values.len(),
                edge(IterationPhiIncomingKind::Fallthrough).bindings().len()
            );
            forward_sources(
                IterationPhiIncomingKind::Fallthrough,
                &back_values,
                &mut source_instances,
            );
            let instance = owners.remove(&back_root.source()).unwrap();
            assert_eq!(back_values[&header.owner()], (back_root.source(), instance));
            assert!(owners.insert(header.owner(), instance).is_none());
            let exhausted = binding(IterationPhiIncomingKind::Exhaustion, exit.owner());
            let selected_values = exhausted
                .values()
                .iter()
                .filter(|value| selected(&table, value.condition(), &choices))
                .collect::<Vec<_>>();
            let [exit_value] = selected_values.as_slice() else {
                panic!("one exhaustion owner value must be selected")
            };
            let selected_roots = exhausted
                .root_sources()
                .iter()
                .filter(|root| {
                    root.source() == exit_value.source()
                        && root.node() == instance_nodes[&owners[&exit_value.source()]]
                        && selected(&table, root.condition(), &choices)
                })
                .collect::<Vec<_>>();
            let [exit_root] = selected_roots.as_slice() else {
                panic!("one exhaustion root must be selected")
            };
            assert_eq!(exit_root.source(), header.owner());
            for (carried_symbol, carried_owner) in &mut carried_roots {
                let carry_exit = phis
                    .iter()
                    .find(|phi| {
                        phi.boundary() == IterationPhiBoundary::Exit
                            && phi.symbol() == *carried_symbol
                    })
                    .unwrap();
                let carry_exhausted =
                    binding(IterationPhiIncomingKind::Exhaustion, carry_exit.owner());
                let [source] = carry_exhausted.values() else {
                    panic!("carried root needs one exhaustion source")
                };
                assert_eq!(source.source(), *carried_owner);
                assert!(selected(&table, source.condition(), &choices));
            }
            let exit_values = replay_captured_edge(
                &table,
                graph,
                phis,
                edge(IterationPhiIncomingKind::Exhaustion),
                &edge_values(&owners, &source_instances),
                &instance_nodes,
                &captures,
                &mut choices,
            );
            assert_eq!(
                exit_values.len(),
                edge(IterationPhiIncomingKind::Exhaustion).bindings().len()
            );
            forward_sources(
                IterationPhiIncomingKind::Exhaustion,
                &exit_values,
                &mut source_instances,
            );
            for (carried_symbol, carried_owner) in &mut carried_roots {
                let carry_exit = phis
                    .iter()
                    .find(|phi| {
                        phi.boundary() == IterationPhiBoundary::Exit
                            && phi.symbol() == *carried_symbol
                    })
                    .unwrap();
                let instance = owners.remove(carried_owner).unwrap();
                assert_eq!(exit_values[&carry_exit.owner()], (*carried_owner, instance));
                assert!(owners.insert(carry_exit.owner(), instance).is_none());
                *carried_owner = carry_exit.owner();
                exit_roots.push(carry_exit.owner());
            }
            let instance = owners.remove(&exit_root.source()).unwrap();
            assert_eq!(exit_values[&exit.owner()], (exit_root.source(), instance));
            assert!(owners.insert(exit.owner(), instance).is_none());
            exit_roots.push(exit.owner());
            let parent_position = table.capture_slot_value(*parent_slot).unwrap().position();
            assert!(
                chains
                    .iter()
                    .all(|(position, _, _)| *position != parent_position)
            );
            chains.push((parent_position, seed, new_instance));
            carried_roots.push((symbol, exit.owner()));
        }
        chains.sort_by_key(|(position, _, _)| *position);
        assert_eq!(chains.len(), 2);
        let borrowed_instance = borrowed_child.map(
            |(symbol, closure, owner, snapshot, input, position, source_drop)| {
                let CleanupCaptureValue::Owner(source_owner) = input.value() else {
                    panic!("shared capture must read its source owner")
                };
                if !source_instances.contains_key(&source_owner) {
                    let Some(CleanupOwnerValue::Expression { expression, .. }) =
                        table.owner_value(source_owner)
                    else {
                        panic!("new shared source must originate from its evaluated expression")
                    };
                    assert_eq!(
                        sources.slice(parsed.ast().expressions().get(*expression).unwrap().span()),
                        Ok("listOf(1)")
                    );
                    next_instance += 1;
                    assert!(
                        source_instances
                            .insert(source_owner, next_instance)
                            .is_none()
                    );
                }
                if expect_source_guard_copy {
                    assert_eq!(source_drop.owner(), Some(source_owner));
                }
                let source_instance = source_instances[&source_owner];
                next_instance += 1;
                let instance = next_instance;
                assert!(instances.insert(instance, closure).is_none());
                assert!(owners.insert(owner, instance).is_none());
                assert!(selected(&table, input.condition(), &choices));
                assert!(
                    shared_loans
                        .insert((instance, position), source_instance)
                        .is_none()
                );
                let [source] = table.owner_snapshot(snapshot).unwrap().capture_inputs() else {
                    panic!("borrowed child snapshot must read its formed owner")
                };
                assert_eq!(source.owner(), owner);
                assert!(selected(&table, source.condition(), &choices));
                assert_eq!(owners.remove(&owner), Some(instance));
                assert!(owners.insert(snapshot, instance).is_none());
                replay_snapshot_choices(&table, snapshot, &mut choices);
                assert!(steps.iter().any(|(_, action)| matches!(
                    action,
                    IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                        if *owner == snapshot && *target == symbol
                )));
                carried_roots.push((symbol, snapshot));
                instance
            },
        );
        for (_, input) in &parent_saves {
            let ClosureCaptureSource::Symbol(symbol) = input.source() else {
                unreachable!("checked above")
            };
            let source = carried_roots
                .iter()
                .find(|(carried_symbol, _)| *carried_symbol == symbol)
                .unwrap()
                .1;
            assert_eq!(input.value(), CleanupCaptureValue::Owner(source));
        }
        next_instance += 1;
        let parent_instance = next_instance;
        assert!(instances.insert(parent_instance, closure).is_none());
        assert!(owners.insert(outer, parent_instance).is_none());
        for (target, input) in &parent_saves {
            assert!(selected(&table, input.condition(), &choices));
            let CleanupCaptureValue::Owner(source) = input.value() else {
                panic!("parent must read each loop exit instance")
            };
            let child = owners.remove(&source).unwrap();
            let position = table.capture_slot_value(*target).unwrap().position();
            assert!(
                captures
                    .insert((parent_instance, position), child)
                    .is_none()
            );
        }
        let (parent_snapshot_index, parent_snapshot, parent_snapshot_guard) = steps
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveOwnerSnapshot {
                    condition,
                    owner,
                    value,
                } if *value == closure => Some((index, *owner, *condition)),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            steps[parent_snapshot_index].0,
            DropPoint::AfterExpression(closure)
        );
        assert!(parent_snapshot_guard.is_none_or(|guard| selected(&table, guard, &choices)));
        let parent_creates = steps
            .iter()
            .enumerate()
            .filter_map(|(index, (point, action))| {
                matches!(action,
                IterationCleanupAction::CreateClosureOwner { owner, closure: value }
                    if *owner == outer && *value == closure)
                .then_some((index, *point))
            })
            .collect::<Vec<_>>();
        let [(parent_create_index, parent_create_point)] = parent_creates.as_slice() else {
            panic!("parent must have one CreateClosureOwner: {parent_creates:?}")
        };
        assert_eq!(*parent_create_point, DropPoint::AfterExpression(closure));
        assert!(*parent_create_index < parent_snapshot_index);
        let parent_capture_steps = steps
            .iter()
            .enumerate()
            .filter_map(|(index, (point, action))| {
                matches!(action,
                IterationCleanupAction::SaveClosureCapture { owner, .. }
                    if *owner == outer)
                .then_some((index, *point))
            })
            .collect::<Vec<_>>();
        assert_eq!(parent_capture_steps.len(), parent_saves.len());
        assert!(parent_capture_steps.iter().all(|(index, point)| {
            *point == DropPoint::AfterExpression(closure)
                && *parent_create_index < *index
                && *index < parent_snapshot_index
        }));
        let [source] = table
            .owner_snapshot(parent_snapshot)
            .unwrap()
            .capture_inputs()
        else {
            panic!("parent snapshot must read the formed environment")
        };
        assert_eq!(source.owner(), outer);
        assert!(selected(&table, source.condition(), &choices));
        let instance = owners.remove(&source.owner()).unwrap();
        assert!(owners.insert(parent_snapshot, instance).is_none());
        let source_guard_copy = borrowed_child
            .and_then(|(_, _, _, _, _, _, source_drop)| source_drop.condition())
            .and_then(|guard| match table.get(guard) {
                Some(CleanupCondition::Choice { selector, .. }) => Some(*selector),
                _ => None,
            })
            .and_then(|selector| {
                table
                    .owner_snapshot(parent_snapshot)
                    .unwrap()
                    .copies()
                    .iter()
                    .copied()
                    .find(|copy| copy.target() == selector)
            });
        assert_eq!(source_guard_copy.is_some(), expect_source_guard_copy);
        let copied_source_value = source_guard_copy.map(|copy| {
            assert!(selected(&table, copy.when(), &choices));
            assert!(copy.source_value().is_none());
            assert!(!choices.contains_key(&copy.target()));
            (copy.target(), choices[&copy.source()])
        });
        replay_snapshot_choices(&table, parent_snapshot, &mut choices);
        if let Some((target, expected)) = copied_source_value {
            assert_eq!(choices[&target], expected);
        }
        let parent_commit_index = steps
            .iter()
            .position(|(_, action)| {
                matches!(
                    action,
                    IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                        if *owner == parent_snapshot && *target == outer_symbol
                )
            })
            .unwrap();
        assert!(parent_commit_index > parent_snapshot_index);
        assert_eq!(
            steps[parent_commit_index].0,
            DropPoint::AfterExpression(closure)
        );
        let binding_instance = owners.remove(&parent_snapshot).unwrap();
        assert_eq!(binding_instance, parent_instance);
        assert_eq!(exit_roots.len(), 3);
        for owner in exit_roots {
            assert!(steps.iter().all(|(_, action)| {
                let root = match action {
                    IterationCleanupAction::Drop(root)
                    | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                    _ => return true,
                };
                root.owner() != Some(owner)
                    || root
                        .condition()
                        .is_some_and(|condition| !selected(&table, condition, &choices))
            }));
        }
        for (symbol, owner) in &carried_roots {
            assert!(steps.iter().all(|(_, action)| {
                let root = match action {
                    IterationCleanupAction::Drop(root)
                    | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                    _ => return true,
                };
                (root.owner() != Some(*owner) && root.target() != DropTarget::Named(*symbol))
                    || root
                        .condition()
                        .is_some_and(|condition| !selected(&table, condition, &choices))
            }));
        }
        let [(layout, root)] = outer_actions.as_slice() else {
            unreachable!("checked above")
        };
        assert_eq!(*layout, ClosureReleaseLayout::File);
        assert!(
            root.condition()
                .is_none_or(|guard| selected(&table, guard, &choices))
        );
        enum ReleaseStep {
            Enter(usize),
            EndLoan(usize, usize),
            Finish(usize),
        }
        let mut pending = vec![ReleaseStep::Enter(binding_instance)];
        let mut visited = BTreeSet::new();
        let mut released = Vec::new();
        let mut ended_loans = Vec::new();
        let mut remaining_source_loans = usize::from(borrowed_instance.is_some());
        while let Some(step) = pending.pop() {
            match step {
                ReleaseStep::Enter(instance) => {
                    assert!(visited.insert(instance), "an instance was released twice");
                    pending.push(ReleaseStep::Finish(instance));
                    for (position, capture) in checked.captures_of(instances[&instance]).enumerate()
                    {
                        match (capture.mode(), capture.effect()) {
                            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move) => {
                                let child = captures.remove(&(instance, position)).unwrap();
                                pending.push(ReleaseStep::Enter(child));
                            }
                            (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow) => {
                                assert_eq!(Some(instance), borrowed_instance);
                                pending.push(ReleaseStep::EndLoan(instance, position));
                            }
                            other => panic!("unexpected capture in File replay: {other:?}"),
                        }
                    }
                }
                ReleaseStep::EndLoan(instance, position) => {
                    let Some((_, _, _, _, input, _, _)) = borrowed_child else {
                        unreachable!("only the borrowed child has a shared loan")
                    };
                    let CleanupCaptureValue::Owner(source_owner) = input.value() else {
                        panic!("shared loan must refer to a source owner")
                    };
                    assert_eq!(
                        shared_loans.remove(&(instance, position)),
                        Some(source_instances[&source_owner])
                    );
                    assert!(!released.contains(&instance));
                    assert!(remaining_source_loans > 0);
                    remaining_source_loans -= 1;
                    ended_loans.push(instance);
                }
                ReleaseStep::Finish(instance) => released.push(instance),
            }
        }
        let mut expected = Vec::new();
        if let Some(instance) = borrowed_instance {
            expected.push(instance);
        }
        expected.extend([
            chains[1].1,
            chains[1].2,
            chains[0].1,
            chains[0].2,
            parent_instance,
        ]);
        assert_eq!(released, expected);
        assert_eq!(
            ended_loans,
            borrowed_instance.into_iter().collect::<Vec<_>>()
        );
        if let Some((_, _, _, _, _, _, source_drop)) = borrowed_child {
            assert_eq!(remaining_source_loans, 0);
            assert_eq!(released.last(), Some(&parent_instance));
            assert!(
                source_drop
                    .condition()
                    .is_none_or(|guard| selected(&table, guard, &choices))
            );
            assert!(
                source_instances
                    .remove(&source_drop.owner().unwrap())
                    .is_some()
            );
        }
        assert!(
            owners.is_empty()
                && source_instances.is_empty()
                && captures.is_empty()
                && shared_loans.is_empty()
        );
    }

    #[test]
    fn parent_of_two_recursive_loops_needs_file_wide_release_layout() {
        assert_cross_loop_parent_release(
            "fun run(first: List<Int>, second: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in first) { f = move { f() } }\nvar g: move () -> Unit = move {}\nfor (_ in second) { g = move { g() } }\nval outer: move () -> Unit = move { val x = f()\nval y = g() }\nval used = outer() }",
            "move { val x = f()\nval y = g() }",
            2,
            "outer",
            false,
            true,
            false,
        );
    }

    #[test]
    fn file_parent_with_recursive_chains_and_shared_child_keeps_one_release_root() {
        assert_cross_loop_parent_release(
            "fun read(xs: List<Int>) {}\nfun run(first: List<Int>, second: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in first) { f = move { f() } }\nvar h: move () -> Unit = move {}\nfor (_ in second) { h = move { h() } }\nval xs = listOf(1)\nval g: () -> Unit = { read(xs) }\nval outer: move () -> Unit = move { val a = f()\nval b = h()\nval c = g() }\nval used = outer() }",
            "move { val a = f()\nval b = h()\nval c = g() }",
            3,
            "outer",
            false,
            true,
            false,
        );
    }

    #[test]
    fn file_parent_shared_source_crosses_two_recursive_loops() {
        assert_cross_loop_parent_release(
            "fun read(xs: List<Int>) {}\nfun run(first: List<Int>, second: List<Int>) {\nval xs = listOf(1)\nvar f: move () -> Unit = move {}\nfor (_ in first) { f = move { f() } }\nvar h: move () -> Unit = move {}\nfor (_ in second) { h = move { h() } }\nval g: () -> Unit = { read(xs) }\nval outer: move () -> Unit = move { val a = f()\nval b = h()\nval c = g() }\nval used = outer() }",
            "move { val a = f()\nval b = h()\nval c = g() }",
            3,
            "outer",
            false,
            true,
            true,
        );
    }

    #[test]
    fn parent_of_recursive_snapshot_needs_file_wide_release_layout() {
        assert_cross_loop_parent_release(
            "fun run(first: List<Int>, second: List<Int>, pick: Boolean) {\nvar f: move () -> Unit = move {}\nfor (_ in first) { f = move { f() } }\nvar g: move () -> Unit = move {}\nfor (_ in second) { g = move { g() } }\nvar h: move () -> Unit = if (pick) f else g\nval outer: move () -> Unit = move { h() }\nval used = outer() }",
            "move { h() }",
            1,
            "outer",
            false,
            true,
            false,
        );
    }

    #[test]
    fn nested_environment_capture_needs_file_wide_release_layout() {
        assert_cross_loop_parent_release(
            "fun run(flags: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in flags) { f = move { f() } }\nval outer: move () -> Unit = move { val inner: move () -> Unit = move { val x = f() }\nval used = inner() }\nval used = outer() }",
            "move { val x = f() }",
            1,
            "inner",
            true,
            true,
            false,
        );
    }

    #[test]
    fn nested_environment_capture_of_ordinary_sibling_keeps_ordinary_drop() {
        assert_cross_loop_parent_release(
            "fun run(flags: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in flags) { f = move { f() } }\nval g: move () -> Unit = move {}\nval outer: move () -> Unit = move { val inner: move () -> Unit = move { val x = g() }\nval used = inner()\nval recursive = f() }\nval used = outer() }",
            "move { val x = g() }",
            1,
            "inner",
            true,
            false,
            false,
        );
    }

    #[test]
    fn snapshot_keeps_two_recursive_phi_roots_without_tree_origins() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-only-roots.ko",
                "fun run(first: List<Int>, second: List<Int>, pick: Boolean) {
var f: move () -> Unit = move {}
var g: move () -> Unit = move {}
for (_ in first) { { f = move { f() } }
g = move { g() } }
var h: move () -> Unit = if (pick) f else g
for (_ in second) { h = move { h() } }
val used = h() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        let expected_actions = planner.cleanup.clone();
        let expected_conditions = planner.conditions.clone();
        let expected_drops = planner.facts.clone();
        let expected_loan_ends = planner.loan_ends.clone();
        let expected_graphs = planner.loop_capture_graphs.clone();
        let expected_phis = planner.loop_phis.clone();
        let expected_incomings = planner.loop_phi_incomings.clone();
        let candidate = planner.into_candidate_facts();
        assert_eq!(candidate.cleanup_steps, expected_actions);
        assert_eq!(candidate.cleanup_conditions, expected_conditions);
        assert_eq!(candidate.drops, expected_drops);
        assert_eq!(candidate.loan_ends, expected_loan_ends);
        assert_eq!(candidate.iterations.len(), expected_graphs.len());
        assert_eq!(candidate.iterations.len(), 2);
        let statements = candidate
            .iterations
            .iter()
            .map(|plan| plan.descriptor().statement().index())
            .collect::<BTreeSet<_>>();
        assert_eq!(statements.len(), candidate.iterations.len());
        assert_eq!(statements, expected_graphs.keys().copied().collect());
        for plan in &candidate.iterations {
            let statement = plan.descriptor().statement().index();
            assert_eq!(plan.capture_graph(), &expected_graphs[&statement]);
            assert_eq!(plan.closure_phis(), expected_phis[&statement]);
            assert_eq!(plan.closure_phi_incomings(), expected_incomings[&statement]);
            for kind in [
                IterationPhiIncomingKind::Entry,
                IterationPhiIncomingKind::Fallthrough,
                IterationPhiIncomingKind::Exhaustion,
            ] {
                assert!(
                    plan.closure_phi_incomings()
                        .iter()
                        .any(|edge| edge.kind() == kind)
                );
            }
        }
        let plans = candidate
            .iterations
            .iter()
            .map(|plan| (plan.descriptor().statement().index(), plan))
            .collect::<BTreeMap<_, _>>();
        let table = &candidate.cleanup_conditions;
        let steps = &candidate.cleanup_steps;
        let second = checker
            .iterations
            .values()
            .map(|plan| plan.descriptor().statement())
            .last()
            .unwrap();
        let h = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("h"))
            .unwrap()
            .id();
        let h_header = plans[&second.index()]
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == h)
            .unwrap();
        let edge = plans[&second.index()]
            .closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
        let entry = edge
            .bindings()
            .iter()
            .find(|binding| binding.target() == h_header.owner())
            .unwrap();
        assert_eq!(entry.values().len(), 1);
        let snapshot = entry.values()[0].source();
        let saved = table.owner_snapshot(snapshot).unwrap();
        assert!(saved.capture_inputs().is_empty());
        assert_eq!(saved.value_inputs().len(), 2);
        let first = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let expected_roots = ["f", "g"]
            .into_iter()
            .map(|name| {
                let symbol = names
                    .symbols()
                    .iter()
                    .find(|symbol| sources.slice(symbol.span()) == Ok(name))
                    .unwrap()
                    .id();
                plans[&first.index()]
                    .closure_phis()
                    .iter()
                    .find(|phi| {
                        phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == symbol
                    })
                    .unwrap()
                    .owner()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            saved
                .value_inputs()
                .iter()
                .map(|input| input.owner())
                .collect::<BTreeSet<_>>(),
            expected_roots.iter().copied().collect()
        );
        assert_eq!(
            (*table).clone().and(
                saved.value_inputs()[0].condition(),
                saved.value_inputs()[1].condition(),
            ),
            CleanupConditionId::NEVER
        );
        assert_eq!(entry.root_sources().len(), 4);
        assert!(
            entry
                .root_sources()
                .iter()
                .all(|source| source.source() == snapshot
                    && h_header.root_nodes().contains(&source.node()))
        );
        let pick_selector = saved
            .copies()
            .iter()
            .map(|copy| copy.source())
            .find(|&selector| {
                table.selector(selector).is_some_and(|selector| {
                    sources
                        .slice(selector.origin())
                        .is_ok_and(|text| text.contains("pick"))
                })
            })
            .unwrap();
        let first_edges = plans[&first.index()].closure_phi_incomings();
        let first_edge = |kind| first_edges.iter().find(|edge| edge.kind() == kind).unwrap();
        let first_entry = first_edge(IterationPhiIncomingKind::Entry);
        let first_backedge = first_edge(IterationPhiIncomingKind::Fallthrough);
        let first_exhausted = first_edge(IterationPhiIncomingKind::Exhaustion);
        fn incoming(
            edge: &crate::ownership_checking::IterationPhiIncoming,
            target: CleanupOwnerValueId,
        ) -> &IterationPhiIncomingBinding {
            edge.bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap()
        }
        fn active_root(
            table: &CleanupConditions,
            binding: &IterationPhiIncomingBinding,
            choices: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
            handles: &BTreeMap<CleanupOwnerValueId, usize>,
            instance_nodes: &BTreeMap<usize, usize>,
        ) -> crate::ownership_checking::IterationPhiRootSource {
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), choices))
                .collect::<Vec<_>>();
            let [value] = values.as_slice() else {
                panic!("one owner value must reach this phi binding")
            };
            let roots = binding
                .root_sources()
                .iter()
                .copied()
                .filter(|root| {
                    root.source() == value.source()
                        && root.node() == instance_nodes[&handles[&value.source()]]
                        && selected(table, root.condition(), choices)
                })
                .collect::<Vec<_>>();
            let [root] = roots.as_slice() else {
                panic!("one actual root handle must reach this phi binding")
            };
            *root
        }
        let paths = saved
            .value_inputs()
            .iter()
            .map(|input| {
                let exit = plans[&first.index()]
                    .closure_phis()
                    .iter()
                    .find(|phi| phi.owner() == input.owner())
                    .unwrap();
                let header = plans[&first.index()]
                    .closure_phis()
                    .iter()
                    .find(|phi| {
                        phi.boundary() == IterationPhiBoundary::Header
                            && phi.symbol() == exit.symbol()
                    })
                    .unwrap();
                let initial = incoming(first_entry, header.owner()).values()[0].source();
                let body_snapshot = incoming(first_backedge, header.owner()).values()[0].source();
                let body_owner =
                    table.owner_snapshot(body_snapshot).unwrap().value_inputs()[0].owner();
                assert_eq!(
                    incoming(first_exhausted, exit.owner()).values()[0].source(),
                    header.owner()
                );
                (
                    header.owner(),
                    exit.owner(),
                    initial,
                    body_owner,
                    body_snapshot,
                )
            })
            .collect::<Vec<_>>();
        for &(header, _, initial, body_owner, body_snapshot) in &paths {
            let body_value = table.owner_snapshot(body_snapshot).unwrap().value();
            let initial_create = steps
                .iter()
                .position(|(point, action)| {
                    matches!(action, IterationCleanupAction::CreateClosureOwner { owner, closure }
                        if *owner == initial && *point == DropPoint::AfterExpression(*closure))
                })
                .unwrap();
            let create = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(body_value)
                        && matches!(action, IterationCleanupAction::CreateClosureOwner { owner, closure }
                            if *owner == body_owner && *closure == body_value)
                })
                .unwrap();
            let capture = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(body_value)
                        && matches!(action, IterationCleanupAction::SaveClosureCapture { owner, .. }
                            if *owner == body_owner)
                })
                .unwrap();
            let save = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(body_value)
                        && matches!(action, IterationCleanupAction::SaveOwnerSnapshot { condition: None, owner, value }
                            if *owner == body_snapshot && *value == body_value)
                })
                .unwrap();
            let symbol = plans[&first.index()]
                .closure_phis()
                .iter()
                .find(|phi| phi.owner() == header)
                .unwrap()
                .symbol();
            let commit = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(body_value)
                        && matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                            if *owner == body_snapshot && *target == symbol)
                })
                .unwrap();
            assert!(initial_create < create && create < capture && capture < save && save < commit);
        }
        let graph = plans[&second.index()].capture_graph();
        let first_graph = plans[&first.index()].capture_graph();
        let first_nodes = |instances: &BTreeMap<usize, usize>| {
            instances
                .iter()
                .map(|(&instance, &node)| {
                    let closure = graph.nodes()[node].closure();
                    let first_node = first_graph
                        .nodes()
                        .iter()
                        .position(|candidate| candidate.closure() == closure)
                        .unwrap();
                    (instance, first_node)
                })
                .collect::<BTreeMap<_, _>>()
        };
        let mut next_instance = 0;
        let mut instance_nodes = BTreeMap::new();
        let mut handles = BTreeMap::new();
        let mut form = |owner,
                        handles: &mut BTreeMap<CleanupOwnerValueId, usize>,
                        instance_nodes: &mut BTreeMap<usize, usize>| {
            let closure = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CreateClosureOwner {
                        owner: created,
                        closure,
                    } if *created == owner => Some(*closure),
                    _ => None,
                })
                .unwrap();
            next_instance += 1;
            let node = graph
                .nodes()
                .iter()
                .position(|node| node.closure() == closure)
                .unwrap();
            assert!(instance_nodes.insert(next_instance, node).is_none());
            assert!(handles.insert(owner, next_instance).is_none());
            next_instance
        };
        let mut choices = table
            .nodes()
            .iter()
            .filter_map(|node| match node {
                CleanupCondition::Choice { selector, .. } => Some((*selector, 0)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        for &(_, _, initial, _, _) in &paths {
            form(initial, &mut handles, &mut instance_nodes);
        }
        let entry_nodes = first_nodes(&instance_nodes);
        let entry_roots = paths
            .iter()
            .map(|&(header, _, initial, _, _)| {
                let root = active_root(
                    table,
                    incoming(first_entry, header),
                    &choices,
                    &handles,
                    &entry_nodes,
                );
                assert_eq!(root.source(), initial);
                (header, root)
            })
            .collect::<Vec<_>>();
        let mut captured = BTreeMap::new();
        let entry_transport = replay_captured_edge(
            table,
            first_graph,
            plans[&first.index()].closure_phis(),
            first_entry,
            &handles,
            &entry_nodes,
            &captured,
            &mut choices,
        );
        assert_eq!(entry_transport.len(), paths.len());
        for (header, root) in entry_roots {
            assert_eq!(
                entry_transport[&header],
                (root.source(), handles[&root.source()])
            );
            let moved = handles.remove(&root.source()).unwrap();
            assert_eq!(
                first_graph.nodes()[root.node()].closure(),
                graph.nodes()[instance_nodes[&moved]].closure()
            );
            assert!(handles.insert(header, moved).is_none());
        }
        for _ in 0..2 {
            for &(header, _, _, body_owner, body_snapshot) in &paths {
                let formed = form(body_owner, &mut handles, &mut instance_nodes);
                let (slot, input) = steps
                    .iter()
                    .find_map(|(_, action)| match action {
                        IterationCleanupAction::SaveClosureCapture {
                            owner,
                            target,
                            input,
                        } if *owner == body_owner => Some((*target, *input)),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(input.value(), CleanupCaptureValue::Owner(header));
                assert_eq!(
                    (input.mode(), input.effect()),
                    (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
                );
                assert!(selected(table, input.condition(), &choices));
                let layout = table.capture_slot_value(slot).unwrap();
                assert_eq!(layout.environment(), body_owner);
                assert_eq!(
                    layout.closure(),
                    table.owner_snapshot(body_snapshot).unwrap().value()
                );
                assert_eq!(layout.source(), input.source());
                let position = layout.position();
                let previous = handles.remove(&header).unwrap();
                assert!(captured.insert((formed, position), previous).is_none());
                let snapshot = table.owner_snapshot(body_snapshot).unwrap();
                assert_eq!(snapshot.value_inputs()[0].owner(), body_owner);
                let formed = handles.remove(&body_owner).unwrap();
                assert!(handles.insert(body_snapshot, formed).is_none());
                replay_snapshot_choices(table, body_snapshot, &mut choices);
            }
            let old_nodes = first_nodes(&instance_nodes);
            let writes = paths
                .iter()
                .map(|&(header, _, _, _, body_snapshot)| {
                    let root = active_root(
                        table,
                        incoming(first_backedge, header),
                        &choices,
                        &handles,
                        &old_nodes,
                    );
                    assert_eq!(root.source(), body_snapshot);
                    let instance = handles[&root.source()];
                    assert_eq!(
                        first_graph.nodes()[root.node()].closure(),
                        graph.nodes()[instance_nodes[&instance]].closure()
                    );
                    (header, root.source(), instance)
                })
                .collect::<Vec<_>>();
            let transported = replay_captured_edge(
                table,
                first_graph,
                plans[&first.index()].closure_phis(),
                first_backedge,
                &handles,
                &old_nodes,
                &captured,
                &mut choices,
            );
            assert_eq!(transported.len(), paths.len());
            for (header, source, instance) in writes {
                assert_eq!(transported[&header], (source, instance));
                assert_eq!(handles.remove(&source), Some(instance));
                assert!(handles.insert(header, instance).is_none());
            }
        }
        let old_nodes = first_nodes(&instance_nodes);
        let writes = paths
            .iter()
            .map(|&(header, exit, _, _, _)| {
                let root = active_root(
                    table,
                    incoming(first_exhausted, exit),
                    &choices,
                    &handles,
                    &old_nodes,
                );
                assert_eq!(root.source(), header);
                let instance = handles[&root.source()];
                assert_eq!(
                    first_graph.nodes()[root.node()].closure(),
                    graph.nodes()[instance_nodes[&instance]].closure()
                );
                (exit, root.source(), instance)
            })
            .collect::<Vec<_>>();
        let transported = replay_captured_edge(
            table,
            first_graph,
            plans[&first.index()].closure_phis(),
            first_exhausted,
            &handles,
            &old_nodes,
            &captured,
            &mut choices,
        );
        assert_eq!(transported.len(), paths.len());
        for (exit, source, instance) in writes {
            assert_eq!(transported[&exit], (source, instance));
            assert_eq!(handles.remove(&source), Some(instance));
            assert!(handles.insert(exit, instance).is_none());
        }
        assert_eq!(instance_nodes.len(), 6);
        let first_instance_nodes = instance_nodes.clone();
        let active_releases = |owner, choices: &BTreeMap<_, _>| {
            steps
                .iter()
                .filter_map(|(point, action)| match action {
                    IterationCleanupAction::ReleaseClosureInstances {
                        layout: ClosureReleaseLayout::Iteration(statement),
                        root,
                    } if root.owner() == Some(owner)
                        && root
                            .condition()
                            .is_none_or(|condition| selected(table, condition, choices)) =>
                    {
                        Some((*point, *statement, *root))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let active_root_actions = |owner, choices: &BTreeMap<_, _>| {
            steps
                .iter()
                .filter_map(|(point, action)| {
                    let (root, release) = match action {
                        IterationCleanupAction::Drop(root) => (root, false),
                        IterationCleanupAction::ReleaseClosureInstances { root, .. } => {
                            (root, true)
                        }
                        _ => return None,
                    };
                    (root.owner() == Some(owner)
                        && root
                            .condition()
                            .is_none_or(|condition| selected(table, condition, choices)))
                    .then_some((*point, release))
                })
                .collect::<Vec<_>>()
        };
        let selected_named_actions = |point, symbol: SymbolId, choices: &BTreeMap<_, _>| {
            steps
                .iter()
                .filter_map(|(at, action)| {
                    let root = match action {
                        IterationCleanupAction::Drop(root)
                        | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                        _ => return None,
                    };
                    (*at == point
                        && root.target() == DropTarget::Named(symbol)
                        && root
                            .condition()
                            .is_none_or(|condition| selected(table, condition, choices)))
                    .then_some(*action)
                })
                .collect::<Vec<_>>()
        };
        for pick in 0..2 {
            let mut choices = choices.clone();
            let mut handles = handles.clone();
            let mut captured = captured.clone();
            choices.insert(pick_selector, pick);
            let selected_values = saved
                .value_inputs()
                .iter()
                .filter(|input| selected(table, input.condition(), &choices))
                .collect::<Vec<_>>();
            let [chosen] = selected_values.as_slice() else {
                panic!("one old recursive root must own the evaluated RHS")
            };
            assert_eq!(chosen.owner(), expected_roots[pick]);
            let unselected = saved
                .value_inputs()
                .iter()
                .find(|input| input.owner() != chosen.owner())
                .unwrap();
            let releases = active_releases(unselected.owner(), &choices);
            let [(release_point, release_statement, release_fact)] = releases.as_slice() else {
                panic!("the unselected recursive root must have one release")
            };
            assert_eq!(*release_statement, first);
            assert_eq!(
                *release_point,
                DropPoint::BranchExit {
                    control: saved.value(),
                    branch: pick,
                }
            );
            let unselected_symbol = plans[&first.index()]
                .closure_phis()
                .iter()
                .find(|phi| phi.owner() == unselected.owner())
                .unwrap()
                .symbol();
            assert_eq!(release_fact.target(), DropTarget::Named(unselected_symbol));
            assert_eq!(
                active_root_actions(unselected.owner(), &choices),
                vec![(*release_point, true)]
            );
            assert_eq!(
                selected_named_actions(*release_point, unselected_symbol, &choices),
                vec![IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(first),
                    root: *release_fact,
                }]
            );
            assert!(active_root_actions(chosen.owner(), &choices).is_empty());
            let release_at = steps
                .iter()
                .position(|(point, action)| {
                    point == release_point
                        && matches!(action, IterationCleanupAction::ReleaseClosureInstances { root, .. }
                            if root == release_fact)
                })
                .unwrap();
            let save_at = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(saved.value())
                        && matches!(action, IterationCleanupAction::SaveOwnerSnapshot { condition: None, owner, value }
                            if *owner == snapshot && *value == saved.value())
                })
                .unwrap();
            let commit_at = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(saved.value())
                        && matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                            if *owner == snapshot && *target == h)
                })
                .unwrap();
            assert!(release_at < save_at && save_at < commit_at);
            let unselected_instance = handles.remove(&unselected.owner()).unwrap();
            let release_graph = plans[&release_statement.index()].capture_graph();
            let release_nodes = first_instance_nodes
                .iter()
                .map(|(&instance, &node)| {
                    let closure = graph.nodes()[node].closure();
                    let release_node = release_graph
                        .nodes()
                        .iter()
                        .position(|candidate| candidate.closure() == closure)
                        .unwrap();
                    (instance, release_node)
                })
                .collect::<BTreeMap<_, _>>();
            let released = replay_owned_closure_release(
                release_graph,
                &release_nodes,
                &mut captured,
                unselected_instance,
            );
            assert_eq!(released.len(), 3);
            assert!(released.windows(2).all(|pair| pair[0] < pair[1]));
            let chosen_instance = handles.remove(&chosen.owner()).unwrap();
            assert!(handles.insert(snapshot, chosen_instance).is_none());
            replay_snapshot_choices(table, snapshot, &mut choices);
            let active = entry
                .root_sources()
                .iter()
                .filter(|source| {
                    source.source() == snapshot
                        && source.node() == instance_nodes[&chosen_instance]
                        && selected(table, source.condition(), &choices)
                })
                .collect::<Vec<_>>();
            let [root] = active.as_slice() else {
                panic!("one saved recursive root must reach the second loop")
            };
            let chosen_phi = plans[&first.index()]
                .closure_phis()
                .iter()
                .find(|phi| phi.owner() == chosen.owner())
                .unwrap();
            let selected_closure = chosen_phi.root_origins().last().unwrap().closure();
            let graph = plans[&second.index()].capture_graph();
            assert_eq!(graph.nodes()[root.node()].closure(), selected_closure);
            assert_eq!(root.source(), snapshot);
            let transported = replay_captured_edge(
                table,
                graph,
                plans[&second.index()].closure_phis(),
                edge,
                &handles,
                &instance_nodes,
                &captured,
                &mut choices,
            );
            assert_eq!(transported[&h_header.owner()], (snapshot, chosen_instance));
            let moved = handles.remove(&root.source()).unwrap();
            assert_eq!(moved, chosen_instance);
            assert!(handles.insert(h_header.owner(), moved).is_none());
            assert_eq!(
                choices[&h_header
                    .root_origins()
                    .find(|target| target.node() == root.node())
                    .unwrap()
                    .selector()],
                1
            );
            assert_eq!(
                h_header
                    .root_origins()
                    .filter(|target| choices[&target.selector()] == 1)
                    .count(),
                2
            );
            assert!(active_root_actions(h_header.owner(), &choices).is_empty());
            let second_backedge = plans[&second.index()]
                .closure_phi_incomings()
                .iter()
                .find(|edge| edge.kind() == IterationPhiIncomingKind::Fallthrough)
                .unwrap();
            let back_binding = incoming(second_backedge, h_header.owner());
            assert_eq!(back_binding.values().len(), 1);
            let body_snapshot = back_binding.values()[0].source();
            let body = table.owner_snapshot(body_snapshot).unwrap();
            assert_eq!(body.value_inputs().len(), 1);
            let body_owner = body.value_inputs()[0].owner();
            let body_value = body.value();
            let actions = steps
                .iter()
                .enumerate()
                .filter(|(_, (point, _))| *point == DropPoint::AfterExpression(body_value))
                .collect::<Vec<_>>();
            let action_index = |matches: &dyn Fn(&IterationCleanupAction) -> bool| {
                actions
                    .iter()
                    .find_map(|(index, (_, action))| matches(action).then_some(*index))
                    .unwrap()
            };
            let create = action_index(&|action| {
                matches!(action,
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *owner == body_owner && *closure == body_value)
            });
            let (slot, input) = actions
                .iter()
                .find_map(|(_, (_, action))| match action {
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if *owner == body_owner => Some((*target, *input)),
                    _ => None,
                })
                .unwrap();
            let capture = action_index(&|action| {
                matches!(action,
                IterationCleanupAction::SaveClosureCapture { owner, .. }
                    if *owner == body_owner)
            });
            let save = action_index(&|action| {
                matches!(action,
                IterationCleanupAction::SaveOwnerSnapshot { condition: None, owner, value }
                    if *owner == body_snapshot && *value == body_value)
            });
            let commit = action_index(&|action| {
                matches!(action,
                IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *owner == body_snapshot && *target == h)
            });
            assert!(create < capture && capture < save && save < commit);
            assert_eq!(input.value(), CleanupCaptureValue::Owner(h_header.owner()));
            assert_eq!(
                (input.mode(), input.effect()),
                (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
            );
            let slot_value = table.capture_slot_value(slot).unwrap();
            assert_eq!(slot_value.environment(), body_owner);
            assert_eq!(slot_value.closure(), body_value);
            assert_eq!(slot_value.source(), input.source());
            let h_exit = plans[&second.index()]
                .closure_phis()
                .iter()
                .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == h)
                .unwrap();
            let exhausted = plans[&second.index()]
                .closure_phi_incomings()
                .iter()
                .find(|edge| edge.kind() == IterationPhiIncomingKind::Exhaustion)
                .unwrap();
            let exit_binding = incoming(exhausted, h_exit.owner());
            assert_eq!(exit_binding.values()[0].source(), h_header.owner());
            let final_call = parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, node)| (sources.slice(node.span()) == Ok("h()")).then_some(id))
                .last()
                .unwrap();
            for rounds in [0, 2] {
                let mut choices = choices.clone();
                let mut handles = handles.clone();
                let mut captured = captured.clone();
                let mut released = released.clone();
                for _ in 0..rounds {
                    assert!(active_root_actions(h_header.owner(), &choices).is_empty());
                    assert!(selected(table, input.condition(), &choices));
                    let formed = form(body_owner, &mut handles, &mut instance_nodes);
                    assert!(active_root_actions(body_owner, &choices).is_empty());
                    let previous = handles.remove(&h_header.owner()).unwrap();
                    assert!(
                        captured
                            .insert((formed, slot_value.position()), previous)
                            .is_none()
                    );
                    let formed = handles.remove(&body_owner).unwrap();
                    assert!(handles.insert(body_snapshot, formed).is_none());
                    replay_snapshot_choices(table, body_snapshot, &mut choices);
                    assert!(active_root_actions(body_owner, &choices).is_empty());
                    assert!(active_root_actions(body_snapshot, &choices).is_empty());
                    assert!(active_root_actions(h_header.owner(), &choices).is_empty());
                    let back_root =
                        active_root(table, back_binding, &choices, &handles, &instance_nodes);
                    assert_eq!(back_root.source(), body_snapshot);
                    assert_eq!(
                        graph.nodes()[back_root.node()].closure(),
                        graph.nodes()[instance_nodes[&formed]].closure()
                    );
                    let transported = replay_captured_edge(
                        table,
                        graph,
                        plans[&second.index()].closure_phis(),
                        second_backedge,
                        &handles,
                        &instance_nodes,
                        &captured,
                        &mut choices,
                    );
                    assert_eq!(transported[&h_header.owner()], (body_snapshot, formed));
                    let moved = handles.remove(&back_root.source()).unwrap();
                    assert_eq!(moved, formed);
                    assert!(handles.insert(h_header.owner(), moved).is_none());
                    assert!(active_root_actions(h_header.owner(), &choices).is_empty());
                }
                let exit_root =
                    active_root(table, exit_binding, &choices, &handles, &instance_nodes);
                assert_eq!(exit_root.source(), h_header.owner());
                let root_instance = handles[&exit_root.source()];
                assert_eq!(
                    graph.nodes()[exit_root.node()].closure(),
                    graph.nodes()[instance_nodes[&root_instance]].closure()
                );
                let transported = replay_captured_edge(
                    table,
                    graph,
                    plans[&second.index()].closure_phis(),
                    exhausted,
                    &handles,
                    &instance_nodes,
                    &captured,
                    &mut choices,
                );
                assert_eq!(
                    transported[&h_exit.owner()],
                    (h_header.owner(), root_instance)
                );
                assert_eq!(handles.remove(&exit_root.source()), Some(root_instance));
                assert!(handles.insert(h_exit.owner(), root_instance).is_none());
                let releases = active_releases(h_exit.owner(), &choices);
                let [(release_point, release_statement, release_fact)] = releases.as_slice() else {
                    panic!("the exit root must have one release")
                };
                assert_eq!(*release_statement, second);
                assert_eq!(*release_point, DropPoint::CallReturn(final_call));
                assert_eq!(release_fact.target(), DropTarget::Named(h));
                assert_eq!(
                    active_root_actions(h_exit.owner(), &choices),
                    vec![(*release_point, true)]
                );
                assert_eq!(
                    selected_named_actions(*release_point, h, &choices),
                    vec![IterationCleanupAction::ReleaseClosureInstances {
                        layout: ClosureReleaseLayout::Iteration(second),
                        root: *release_fact,
                    }]
                );
                released.extend(replay_owned_closure_release(
                    plans[&release_statement.index()].capture_graph(),
                    &instance_nodes,
                    &mut captured,
                    handles.remove(&h_exit.owner()).unwrap(),
                ));
                assert_eq!(released.len(), 6 + rounds);
                assert!(released[3..].windows(2).all(|pair| pair[0] < pair[1]));
                assert_eq!(
                    released.into_iter().collect::<BTreeSet<_>>().len(),
                    6 + rounds
                );
                assert!(captured.is_empty() && handles.is_empty());
            }
        }
    }

    #[test]
    fn next_loop_entry_keeps_recursive_root_after_conditional_snapshot() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-snapshot-next-loop.ko",
                "fun run(first: List<Int>, second: List<Int>, pick: Boolean) {
var f: move () -> Unit = move {}
for (_ in first) { f = move { f() } }
var g: move () -> Unit = if (pick) f else (move {})
for (_ in second) { g = move { g() } }
val used = g() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        let statements = checker
            .iterations
            .values()
            .map(|plan| plan.descriptor().statement())
            .collect::<Vec<_>>();
        assert_eq!(statements.len(), 2);
        let f = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("f"))
            .unwrap()
            .id();
        let first_exit = planner.loop_phis[&statements[0].index()]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == f)
            .unwrap();
        let g = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("g"))
            .unwrap()
            .id();
        let second_header = planner.loop_phis[&statements[1].index()]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == g)
            .unwrap();
        let entry = planner.loop_phi_incomings[&statements[1].index()]
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap()
            .bindings()
            .iter()
            .find(|binding| binding.target() == second_header.owner())
            .unwrap();
        assert_eq!(entry.values().len(), 1);
        let snapshot = entry.values()[0].source();
        let saved = planner.conditions.owner_snapshot(snapshot).unwrap();
        let graph = &planner.loop_capture_graphs[&statements[1].index()];
        let mut conditions = planner.conditions.clone();
        for old_root in first_exit.root_origins() {
            assert!(
                saved
                    .copies()
                    .iter()
                    .any(|copy| copy.source() == old_root.selector())
            );
            let node = *second_header
                .root_nodes()
                .iter()
                .find(|&&node| graph.nodes()[node].closure() == old_root.closure())
                .unwrap();
            let root = entry
                .root_sources()
                .iter()
                .find(|source| source.node() == node && source.source() == snapshot)
                .unwrap();
            let saved_root = planner.snapshot_phi_roots[&snapshot]
                .iter()
                .find(|(closure, _, _)| *closure == old_root.closure())
                .unwrap();
            assert_eq!(
                root.condition(),
                conditions.and(entry.values()[0].condition(), saved_root.2)
            );
        }
    }

    #[test]
    fn next_recursive_loop_entry_keeps_previous_phi_root_sources() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-next-loop.ko",
                "fun run(first: List<Int>, second: List<Int>) {
var f: move () -> Unit = move {}
for (_ in first) { f = move { f() } }
for (_ in second) { f = move { f() } }
val used = f() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        let statements = checker
            .iterations
            .values()
            .map(|plan| plan.descriptor().statement())
            .collect::<Vec<_>>();
        assert_eq!(statements.len(), 2);
        let first_exit = planner.loop_phis[&statements[0].index()]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
            .unwrap();
        let second_entry = planner.loop_phi_incomings[&statements[1].index()]
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
        let f = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("f"))
            .unwrap()
            .id();
        let second_header = planner.loop_phis[&statements[1].index()]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == f)
            .unwrap();
        let entry = second_entry
            .bindings()
            .iter()
            .find(|binding| binding.target() == second_header.owner())
            .unwrap();
        assert_eq!(entry.values().len(), 1);
        assert_eq!(entry.values()[0].source(), first_exit.owner());
        assert!(!entry.root_sources().is_empty());
        assert!(entry.root_sources().iter().all(|source| {
            source.source() == first_exit.owner()
                && second_header.root_nodes().contains(&source.node())
        }));
        let graph = &planner.loop_capture_graphs[&statements[1].index()];
        let mut conditions = planner.conditions.clone();
        for source in entry.root_sources() {
            let closure = graph.nodes()[source.node()].closure();
            let previous = first_exit
                .root_origins()
                .find(|root| root.closure() == closure)
                .unwrap();
            assert_eq!(
                source.condition(),
                conditions.and(entry.values()[0].condition(), previous.condition())
            );
        }
    }

    #[test]
    fn moved_recursive_phi_releases_instances_through_new_binding() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-moved-owner.ko",
                "fun run(flags: List<Int>) {
var f: move () -> Unit = move {}
for (_ in flags) { f = move { f() } }
val h: move () -> Unit = f
val used = h() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        let h = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("h"))
            .unwrap()
            .id();
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let releases = planner
            .cleanup
            .iter()
            .filter(|(_, action)| {
                matches!(action, IterationCleanupAction::ReleaseClosureInstances {
                layout: ClosureReleaseLayout::Iteration(release_statement),
                root,
            } if *release_statement == statement && root.target() == DropTarget::Named(h))
            })
            .count();
        assert_eq!(releases, 1);
        assert!(!planner.cleanup.iter().any(|(_, action)| {
            matches!(action, IterationCleanupAction::Drop(fact) if fact.target() == DropTarget::Named(h))
        }));
    }

    #[test]
    fn conditional_snapshot_of_recursive_phi_keeps_instance_release() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-snapshot-owner.ko",
                "fun run(flags: List<Int>, pick: Boolean) {
var f: move () -> Unit = move {}
for (_ in flags) { f = move { f() } }
val h: move () -> Unit = if (pick) f else (move {})
val used = h() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        let h = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("h"))
            .unwrap()
            .id();
        let release = planner.cleanup.iter().find_map(|(_, action)| match action {
            IterationCleanupAction::ReleaseClosureInstances { root, .. }
                if root.target() == DropTarget::Named(h) =>
            {
                Some(*root)
            }
            _ => None,
        });
        let release = release.expect("the recursive branch retains instance release");
        let ordinary = planner.cleanup.iter().find_map(|(_, action)| match action {
            IterationCleanupAction::Drop(fact) if fact.target() == DropTarget::Named(h) => {
                Some(*fact)
            }
            _ => None,
        });
        let ordinary = ordinary.expect("the other branch retains ordinary cleanup");
        assert_eq!(
            planner.conditions.and(
                release.condition().unwrap_or(CleanupConditionId::ALWAYS),
                ordinary.condition().unwrap_or(CleanupConditionId::ALWAYS)
            ),
            CleanupConditionId::NEVER
        );
    }

    #[test]
    fn conditional_snapshot_of_independent_phi_does_not_duplicate_child_drop() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-independent-snapshot.ko",
                "fun run(flags: List<Int>, pick: Boolean) {
var f: move () -> Unit = move {}
val leaf: move () -> Unit = move {}
var g: move () -> Unit = move { leaf() }
for (_ in flags) { f = move { f() } }
val h: move () -> Unit = if (pick) g else (move {})
val a = f()
val b = h() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        let h = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("h"))
            .unwrap()
            .id();
        let root_drops = planner
            .cleanup
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::Drop(fact) if fact.target() == DropTarget::Named(h) => {
                    Some((*point, *fact))
                }
                _ => None,
            });
        let root_drops = root_drops.collect::<Vec<_>>();
        assert!(!root_drops.is_empty());
        assert!(!planner.cleanup.iter().any(|(_, action)| matches!(action,
            IterationCleanupAction::ReleaseClosureInstances { root, .. }
                if root.target() == DropTarget::Named(h))));
        let point = root_drops[0].0;
        let children = planner
            .cleanup
            .iter()
            .filter(|(drop_point, action)| {
                matches!(action, IterationCleanupAction::Drop(fact)
                if *drop_point == point
                    && matches!(fact.target(), DropTarget::Captured { .. })
                    && sources.slice(fact.value_origin()) == Ok("leaf"))
            })
            .collect::<Vec<_>>();
        assert_eq!(children.len(), 1);
        let IterationCleanupAction::Drop(child) = children[0].1 else {
            unreachable!()
        };
        let mut conditions = planner.conditions.clone();
        let covered = root_drops
            .iter()
            .fold(CleanupConditionId::NEVER, |covered, (_, fact)| {
                conditions.or(
                    covered,
                    fact.condition().unwrap_or(CleanupConditionId::ALWAYS),
                )
            });
        let uncovered = conditions.not(covered);
        assert_eq!(
            conditions.and(
                child.condition().unwrap_or(CleanupConditionId::ALWAYS),
                uncovered
            ),
            CleanupConditionId::NEVER
        );
    }

    #[test]
    fn captured_phi_release_does_not_duplicate_child_drop() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-captured-owner.ko",
                "fun run(flags: List<Int>) {
var f: move () -> Unit = move {}
val leaf: move () -> Unit = move {}
var g: move () -> Unit = move { leaf() }
for (_ in flags) { f = move { f() } }
val outer: move () -> Unit = move { val used = g() }
val used = outer()
val also = f() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        let g = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("g"))
            .unwrap()
            .id();
        let captured_drop = planner.cleanup.iter().find_map(|(point, action)| {
            let IterationCleanupAction::Drop(fact) = action else {
                return None;
            };
            match planner.conditions.owner_value(fact.owner()?) {
                Some(CleanupOwnerValue::IterationPhi { symbol, .. })
                    if *symbol == g && matches!(fact.target(), DropTarget::Captured { .. }) =>
                {
                    Some((*point, *fact))
                }
                _ => None,
            }
        });
        let (point, _) = captured_drop.expect("captured independent g keeps its ordinary drop");
        let children = planner
            .cleanup
            .iter()
            .filter(|(drop_point, action)| {
                matches!(action, IterationCleanupAction::Drop(fact)
                if *drop_point == point
                    && matches!(fact.target(), DropTarget::Captured { .. })
                    && sources.slice(fact.value_origin()) == Ok("leaf"))
            })
            .collect::<Vec<_>>();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn mixed_phi_and_new_closure_versions_do_not_duplicate_instance_release() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "recursive-mixed-owner.ko",
                "fun run(flags: List<Int>, flag: Boolean) {
var f: move () -> Unit = move {}
val old_leaf: move () -> Unit = move {}
var g: move () -> Unit = move { old_leaf() }
for (_ in flags) { f = move { f() } }
if (flag) {
    val new_leaf: move () -> Unit = move {}
    g = move { new_leaf() }
}
val first = f()
val second = g() }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some());
        let g = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("g"))
            .unwrap()
            .id();
        let old_roots = planner
            .cleanup
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::Drop(fact)
                    if fact.target() == DropTarget::Named(g)
                        && fact.owner().is_some_and(|owner| {
                            matches!(
                                planner.conditions.owner_value(owner),
                                Some(CleanupOwnerValue::IterationPhi { .. })
                            )
                        }) =>
                {
                    Some(*fact)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            !old_roots.is_empty(),
            "the independent g phi keeps its root drop"
        );
        assert!(planner.cleanup.iter().any(|(_, action)| match action {
            IterationCleanupAction::Drop(fact) => {
                sources.slice(fact.value_origin()) == Ok("old_leaf")
            }
            _ => false,
        }));
        assert!(
            planner.cleanup.iter().any(|(_, action)| match action {
                IterationCleanupAction::Drop(fact) => {
                    sources.slice(fact.value_origin()) == Ok("new_leaf")
                }
                _ => false,
            }),
            "the ordinary new closure still releases its captured child"
        );
    }

    #[test]
    fn coexisting_same_lambda_paths_replay_two_rounds_and_release() {
        fn no_flat_capture_writes(origin: &IterationPhiIncomingOrigin) -> bool {
            origin
                .environments()
                .iter()
                .flat_map(|environment| environment.sources())
                .all(|source| {
                    source.target().is_none()
                        && source.capture_slot().is_none()
                        && source.captured().iter().all(no_flat_capture_writes)
                })
        }

        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "same-lambda-children.ko",
                "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>, next: List<Int>) {
                    var first: move () -> Unit = move {}
                    var second: move () -> Unit = move {}
                    for (_ in flags) {
                        second = first
                        val xs = listOf(1)
                        { first = move { read(xs) } }
                    }
                    var outer: move () -> Unit = move { val a = first()\nval b = second() }
                    for (_ in next) {}
                    val used = outer()
                }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.coexisting_capture_phi.is_some());
        let candidate = planner.into_candidate_facts();
        let plans = candidate
            .iterations
            .iter()
            .map(|plan| (plan.descriptor().statement().index(), plan))
            .collect::<BTreeMap<_, _>>();
        let steps = &candidate.cleanup_steps;
        let table = &candidate.cleanup_conditions;
        let (statement, graph, parent) = plans
            .iter()
            .find_map(|(&statement, plan)| {
                let graph = plan.capture_graph();
                graph.nodes().iter().enumerate().find_map(|(index, node)| {
                    (sources.slice(parsed.ast().expressions().get(node.closure()).ok()?.span())
                        == Ok("move { val a = first()\nval b = second() }"))
                    .then_some((statement, graph, index))
                })
            })
            .unwrap();
        let parent = &graph.nodes()[parent];
        assert_eq!(parent.sources().len(), 2);
        let repeated = graph
            .nodes()
            .iter()
            .position(|node| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                ) == Ok("move { read(xs) }")
            })
            .unwrap();
        assert!(
            parent
                .sources()
                .iter()
                .all(|source| source.captured().contains(&repeated))
        );
        assert_eq!(parent.sources()[0].position(), 0);
        assert_eq!(parent.sources()[1].position(), 1);
        assert_eq!(statement, *plans.keys().max().unwrap());
        let formed = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *closure == parent.closure() =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        let saves = steps
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == formed => Some((target, input)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(saves.len(), 2);
        for (index, (target, input)) in saves.iter().enumerate() {
            let slot = table.capture_slot_value(**target).unwrap();
            assert_eq!(slot.environment(), formed);
            assert_eq!(slot.closure(), parent.closure());
            assert_eq!(slot.source(), parent.sources()[index].capture().source());
            assert_eq!(slot.position(), index);
            assert_eq!(input.mode(), ClosureCaptureMode::Owned);
            assert_eq!(input.effect(), ClosureCaptureEffect::Move);
        }
        let first = match saves[0].1.value() {
            CleanupCaptureValue::Owner(owner) => owner,
            other => panic!("first source must retain a phi owner: {other:?}"),
        };
        let second = match saves[1].1.value() {
            CleanupCaptureValue::Owner(owner) => owner,
            other => panic!("second source must retain a phi owner: {other:?}"),
        };
        assert_ne!(first, second);
        let incomings = &plans[&statement].closure_phi_incomings();
        assert!(
            incomings
                .iter()
                .any(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        );
        assert!(
            incomings
                .iter()
                .any(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
        );
        assert!(
            incomings
                .iter()
                .any(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        );
        assert!(
            incomings.iter().all(|incoming| incoming
                .bindings()
                .iter()
                .all(|binding| binding.capture_slots_to_clear().is_empty()
                    && binding.origins().iter().all(no_flat_capture_writes))),
            "coexisting child instances must retain presence without static capture writes"
        );

        // Execute the first loop's published formation and phi root facts twice.
        let first_loop = *plans.keys().min().unwrap();
        let phis = &plans[&first_loop].closure_phis();
        let source_symbol = |index: usize| match parent.sources()[index].capture().source() {
            ClosureCaptureSource::Symbol(symbol) => symbol,
            other => panic!("parent capture must read a named binding: {other:?}"),
        };
        let first_symbol = source_symbol(0);
        let second_symbol = source_symbol(1);
        let phi = |symbol, boundary| {
            phis.iter()
                .find(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
                .unwrap()
        };
        let first_header = phi(first_symbol, IterationPhiBoundary::Header);
        let second_header = phi(second_symbol, IterationPhiBoundary::Header);
        let first_exit = phi(first_symbol, IterationPhiBoundary::Exit);
        let second_exit = phi(second_symbol, IterationPhiBoundary::Exit);
        let first_incomings = &plans[&first_loop].closure_phi_incomings();
        let incoming = |kind| {
            first_incomings
                .iter()
                .find(|incoming| incoming.kind() == kind)
                .unwrap()
        };
        let first_binding = |kind, target| {
            incoming(kind)
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap()
        };
        let first_entry = first_binding(IterationPhiIncomingKind::Entry, first_header.owner());
        let second_entry = first_binding(IterationPhiIncomingKind::Entry, second_header.owner());
        let first_backedge =
            first_binding(IterationPhiIncomingKind::Fallthrough, first_header.owner());
        let second_backedge =
            first_binding(IterationPhiIncomingKind::Fallthrough, second_header.owner());
        let first_exhausted =
            first_binding(IterationPhiIncomingKind::Exhaustion, first_exit.owner());
        let second_exhausted =
            first_binding(IterationPhiIncomingKind::Exhaustion, second_exit.owner());
        for binding in [
            first_entry,
            second_entry,
            first_backedge,
            second_backedge,
            first_exhausted,
            second_exhausted,
        ] {
            assert_eq!(binding.values().len(), 1);
        }
        let create_for = |wanted| {
            steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CreateClosureOwner { owner, .. }
                        if *owner == wanted =>
                    {
                        Some(*action)
                    }
                    _ => None,
                })
                .unwrap()
        };
        let mut next_instance = 0;
        let mut instance_nodes = BTreeMap::new();
        let mut values = BTreeMap::new();
        let mut create = |action,
                          values: &mut BTreeMap<CleanupOwnerValueId, usize>,
                          instance_nodes: &mut BTreeMap<usize, usize>| {
            let IterationCleanupAction::CreateClosureOwner { owner, closure } = action else {
                panic!("an environment must be formed by its Create action")
            };
            next_instance += 1;
            let node = graph
                .nodes()
                .iter()
                .position(|node| node.closure() == closure)
                .unwrap();
            assert!(instance_nodes.insert(next_instance, node).is_none());
            assert!(values.insert(owner, next_instance).is_none());
            next_instance
        };
        let initial_first = create(
            create_for(first_entry.values()[0].source()),
            &mut values,
            &mut instance_nodes,
        );
        let initial_second = create(
            create_for(second_entry.values()[0].source()),
            &mut values,
            &mut instance_nodes,
        );
        let mut choices = BTreeMap::new();
        let selected_root = |binding: &IterationPhiIncomingBinding, choices: &BTreeMap<_, _>| {
            let roots = binding
                .root_sources()
                .iter()
                .filter(|root| selected(&table, root.condition(), choices))
                .collect::<Vec<_>>();
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(&table, value.condition(), choices))
                .collect::<Vec<_>>();
            let ([root], [value]) = (roots.as_slice(), values.as_slice()) else {
                panic!("a present phi binding must select one root and one value")
            };
            assert_eq!(root.source(), value.source());
            **root
        };
        for binding in [first_entry, second_entry] {
            assert_eq!(
                binding.presence_source(),
                IterationPhiPresenceSource::StaticConditions
            );
            assert_eq!(
                selected_root(binding, &choices).source(),
                binding.values()[0].source()
            );
        }
        replay_edge_presence(
            &table,
            incoming(IterationPhiIncomingKind::Entry),
            &mut choices,
        );
        for binding in [first_entry, second_entry] {
            let moved = values.remove(&binding.values()[0].source()).unwrap();
            assert!(values.insert(binding.target(), moved).is_none());
        }
        let snapshot_for = |symbol| {
            steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                        if *target == symbol =>
                    {
                        Some(*owner)
                    }
                    _ => None,
                })
                .unwrap()
        };
        let first_snapshot = snapshot_for(first_symbol);
        let second_snapshot = snapshot_for(second_symbol);
        let inner_create = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { closure, .. }
                    if *closure == graph.nodes()[repeated].closure() =>
                {
                    Some(*action)
                }
                _ => None,
            })
            .unwrap();
        let IterationCleanupAction::CreateClosureOwner {
            owner: inner_owner, ..
        } = inner_create
        else {
            unreachable!()
        };
        let inner_capture = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == inner_owner => Some((*target, *input)),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            table
                .capture_slot_value(inner_capture.0)
                .unwrap()
                .position(),
            0
        );
        assert_eq!(inner_capture.1.mode(), ClosureCaptureMode::Owned);
        assert_eq!(inner_capture.1.effect(), ClosureCaptureEffect::Move);
        let CleanupCaptureValue::Owner(xs_owner) = inner_capture.1.value() else {
            panic!("the repeated inner closure must move its own source")
        };
        let Some(CleanupOwnerValue::Expression { expression: xs, .. }) =
            table.owner_value(xs_owner)
        else {
            panic!("the owned source must come from this round's expression")
        };
        assert_eq!(
            sources.slice(parsed.ast().expressions().get(*xs).unwrap().span()),
            Ok("listOf(1)")
        );
        assert_eq!(
            table
                .owner_snapshot(second_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            first_header.owner()
        );
        assert_eq!(
            table
                .owner_snapshot(first_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            inner_owner
        );
        assert_eq!(first_backedge.values()[0].source(), first_snapshot);
        assert_eq!(second_backedge.values()[0].source(), second_snapshot);
        let step = |wanted| {
            steps
                .iter()
                .position(|(_, action)| *action == wanted)
                .unwrap()
        };
        let snapshot_action = |wanted| {
            steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::SaveOwnerSnapshot { owner, .. } if *owner == wanted => {
                        Some(*action)
                    }
                    _ => None,
                })
                .unwrap()
        };
        let second_drop = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::Drop(fact)
                    if fact.target() == DropTarget::Named(second_symbol)
                        && fact.owner() == Some(second_header.owner()) =>
                {
                    Some(*action)
                }
                _ => None,
            })
            .unwrap();
        let second_replacement_value = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(_, node)| {
                (sources.slice(node.span()).ok()? == "second = first").then(|| {
                    let Expression::Assignment { value, .. } = node.payload() else {
                        panic!("the old second drop belongs to an assignment")
                    };
                    *value
                })
            })
            .unwrap();
        let IterationCleanupAction::Drop(old_second_fact) = second_drop else {
            unreachable!()
        };
        assert_eq!(
            old_second_fact.point(),
            DropPoint::AfterExpression(second_replacement_value)
        );
        assert_eq!(steps[step(second_drop)].0, old_second_fact.point());
        let inner_save = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture { owner, .. }
                    if *owner == inner_owner =>
                {
                    Some(*action)
                }
                _ => None,
            })
            .unwrap();
        assert!(
            step(snapshot_action(second_snapshot)) < step(second_drop)
                && step(second_drop)
                    < step(IterationCleanupAction::CommitOwnerSnapshot {
                        owner: second_snapshot,
                        target: second_symbol,
                    })
                && step(IterationCleanupAction::CommitOwnerSnapshot {
                    owner: second_snapshot,
                    target: second_symbol,
                }) < step(inner_create)
                && step(inner_create) < step(inner_save)
                && step(inner_save) < step(snapshot_action(first_snapshot))
                && step(snapshot_action(first_snapshot))
                    < step(IterationCleanupAction::CommitOwnerSnapshot {
                        owner: first_snapshot,
                        target: first_symbol,
                    })
        );
        let mut leaf_resources = BTreeMap::new();
        let mut dropped_old = Vec::new();
        for round in 1..=2 {
            let IterationCleanupAction::SaveOwnerSnapshot { condition, .. } =
                snapshot_action(second_snapshot)
            else {
                unreachable!()
            };
            assert!(condition.is_none_or(|guard| selected(&table, guard, &choices)));
            let second_inputs = table
                .owner_snapshot(second_snapshot)
                .unwrap()
                .capture_inputs()
                .iter()
                .filter(|input| selected(&table, input.condition(), &choices))
                .collect::<Vec<_>>();
            let [second_input] = second_inputs.as_slice() else {
                panic!("round {round} must select one old second input")
            };
            let second_input = second_input.owner();
            replay_snapshot_choices(&table, second_snapshot, &mut choices);
            let moved = values.remove(&second_input).unwrap();
            assert!(values.insert(second_snapshot, moved).is_none());
            let IterationCleanupAction::Drop(old_second) = second_drop else {
                unreachable!()
            };
            assert!(
                old_second
                    .condition()
                    .is_none_or(|guard| selected(&table, guard, &choices))
            );
            let old = values.remove(&old_second.owner().unwrap()).unwrap();
            assert!(graph.nodes()[instance_nodes[&old]].sources().is_empty());
            dropped_old.push(old);
            let inner = create(inner_create, &mut values, &mut instance_nodes);
            assert_eq!(instance_nodes[&inner], repeated);
            let IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } = inner_save
            else {
                unreachable!()
            };
            assert_eq!(owner, inner_owner);
            assert_eq!((target, input), inner_capture);
            assert!(selected(&table, input.condition(), &choices));
            let CleanupCaptureValue::Owner(source) = input.value() else {
                unreachable!()
            };
            let mut evaluated_source = BTreeMap::from([(xs_owner, 100 + round)]);
            let captured_source = evaluated_source.remove(&source).unwrap();
            assert!(evaluated_source.is_empty());
            let position = table.capture_slot_value(target).unwrap().position();
            assert!(
                leaf_resources
                    .insert((inner, position), captured_source)
                    .is_none()
            );
            let IterationCleanupAction::SaveOwnerSnapshot { condition, .. } =
                snapshot_action(first_snapshot)
            else {
                unreachable!()
            };
            assert!(condition.is_none_or(|guard| selected(&table, guard, &choices)));
            let first_inputs = table
                .owner_snapshot(first_snapshot)
                .unwrap()
                .capture_inputs()
                .iter()
                .filter(|input| selected(&table, input.condition(), &choices))
                .collect::<Vec<_>>();
            let [first_input] = first_inputs.as_slice() else {
                panic!("round {round} must select one new first input")
            };
            let first_input = first_input.owner();
            replay_snapshot_choices(&table, first_snapshot, &mut choices);
            let formed = values.remove(&first_input).unwrap();
            assert_eq!(formed, inner);
            assert!(values.insert(first_snapshot, formed).is_none());
            for binding in [first_backedge, second_backedge] {
                assert_eq!(
                    selected_root(binding, &choices).source(),
                    binding.values()[0].source()
                );
            }
            replay_edge_presence(
                &table,
                incoming(IterationPhiIncomingKind::Fallthrough),
                &mut choices,
            );
            let writes = [first_backedge, second_backedge]
                .into_iter()
                .map(|binding| {
                    (
                        binding.target(),
                        values.remove(&binding.values()[0].source()).unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            for (target, instance) in writes {
                assert!(values.insert(target, instance).is_none());
            }
        }
        assert_eq!(dropped_old, [initial_second, initial_first]);
        let first_child = values[&first_header.owner()];
        let second_child = values[&second_header.owner()];
        assert_ne!(first_child, second_child);
        assert_eq!(instance_nodes[&first_child], repeated);
        assert_eq!(instance_nodes[&second_child], repeated);
        for binding in [first_exhausted, second_exhausted] {
            assert_eq!(
                selected_root(binding, &choices).source(),
                binding.values()[0].source()
            );
        }
        replay_edge_presence(
            &table,
            incoming(IterationPhiIncomingKind::Exhaustion),
            &mut choices,
        );
        for binding in [first_exhausted, second_exhausted] {
            let moved = values.remove(&binding.values()[0].source()).unwrap();
            assert!(values.insert(binding.target(), moved).is_none());
        }
        assert_eq!(values[&first_exit.owner()], first_child);
        assert_eq!(values[&second_exit.owner()], second_child);

        // The second loop must forward the parent handle, never rewrite either saved child edge.
        let parent_create = create_for(formed);
        let parent_instance = create(parent_create, &mut values, &mut instance_nodes);
        let mut saved_children = BTreeMap::new();
        for (target, input) in &saves {
            assert!(selected(&table, input.condition(), &choices));
            let slot = table.capture_slot_value(**target).unwrap();
            let CleanupCaptureValue::Owner(source) = input.value() else {
                unreachable!()
            };
            let child = values.remove(&source).unwrap();
            assert!(
                saved_children
                    .insert((parent_instance, slot.position()), child)
                    .is_none()
            );
        }
        assert_eq!(saved_children[&(parent_instance, 0)], first_child);
        assert_eq!(saved_children[&(parent_instance, 1)], second_child);
        let replay_instance_presence = |edge: &crate::ownership_checking::IterationPhiIncoming,
                                        values: &BTreeMap<CleanupOwnerValueId, usize>,
                                        choices: &mut BTreeMap<
            crate::ownership_checking::CleanupSelectorId,
            usize,
        >| {
            let before = choices.clone();
            assert!(selected(&table, edge.condition(), &before));
            let [binding] = edge.bindings() else {
                panic!("the second loop carries one parent binding")
            };
            assert!(selected(&table, binding.available_when(), &before));
            assert_eq!(
                binding.presence_source(),
                IterationPhiPresenceSource::CapturedInstances
            );
            let roots = binding
                .root_sources()
                .iter()
                .filter(|root| selected(&table, root.condition(), &before))
                .collect::<Vec<_>>();
            let [root] = roots.as_slice() else {
                panic!("the parent must have one selected root instance")
            };
            let selected_values = binding
                .values()
                .iter()
                .filter(|value| selected(&table, value.condition(), &before))
                .collect::<Vec<_>>();
            let [value] = selected_values.as_slice() else {
                panic!("the parent must have one selected value input")
            };
            assert_eq!(root.source(), value.source());
            let instance = values[&value.source()];
            assert_eq!(instance_nodes[&instance], root.node());
            let mut pending = vec![instance];
            let mut visited = BTreeSet::new();
            let mut reached = BTreeSet::new();
            while let Some(instance) = pending.pop() {
                assert!(visited.insert(instance), "a formed child is visited once");
                let node = instance_nodes[&instance];
                reached.insert(node);
                for source in graph.nodes()[node].sources() {
                    if source.captured().is_empty() {
                        continue;
                    }
                    let child = saved_children[&(instance, source.position())];
                    assert!(source.captured().contains(&instance_nodes[&child]));
                    pending.push(child);
                }
            }
            assert_eq!(
                visited,
                BTreeSet::from([parent_instance, first_child, second_child])
            );
            assert_eq!(
                reached,
                BTreeSet::from([instance_nodes[&parent_instance], repeated])
            );
            let layout = plans[&statement]
                .closure_phis()
                .iter()
                .find(|phi| phi.owner() == binding.target())
                .unwrap();
            let expected_writes = layout
                .origins()
                .iter()
                .map(|origin| (origin.node(), origin.selector()))
                .collect::<BTreeMap<_, _>>();
            let actual_writes = binding
                .selector_writes()
                .iter()
                .map(|write| (write.node(), write.target()))
                .collect::<BTreeMap<_, _>>();
            assert_eq!(binding.selector_writes().len(), expected_writes.len());
            assert_eq!(actual_writes, expected_writes);
            let mut writes = vec![(
                binding.availability_selector(),
                usize::from(selected(&table, binding.available_when(), &before)),
            )];
            writes.extend(
                binding
                    .selector_writes()
                    .iter()
                    .map(|write| (write.target(), usize::from(reached.contains(&write.node())))),
            );
            for (target, value) in writes {
                choices.insert(target, value);
            }
            for write in binding.selector_writes() {
                assert_eq!(
                    choices[&write.target()],
                    usize::from(reached.contains(&write.node()))
                );
            }
            (binding.target(), value.source(), instance)
        };
        let parent_snapshot = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                    if *value == parent.closure() =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            table
                .owner_snapshot(parent_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            formed
        );
        let parent_saves = saves
            .iter()
            .map(
                |(target, input)| IterationCleanupAction::SaveClosureCapture {
                    owner: formed,
                    target: **target,
                    input: **input,
                },
            )
            .collect::<Vec<_>>();
        let parent_snapshot_action = snapshot_action(parent_snapshot);
        let header = plans[&statement]
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
            .unwrap();
        let outer_symbol = header.symbol();
        assert_eq!(
            sources.slice(names.symbols()[outer_symbol.index()].span()),
            Ok("outer")
        );
        let parent_commit = IterationCleanupAction::CommitOwnerSnapshot {
            owner: parent_snapshot,
            target: outer_symbol,
        };
        let parent_actions = [
            parent_create,
            parent_saves[0],
            parent_saves[1],
            parent_snapshot_action,
            parent_commit,
        ];
        assert!(
            parent_actions
                .windows(2)
                .all(|pair| step(pair[0]) < step(pair[1]))
        );
        assert!(parent_actions.iter().all(|action| {
            steps[step(*action)].0 == DropPoint::AfterExpression(parent.closure())
        }));
        let IterationCleanupAction::SaveOwnerSnapshot { condition, .. } = parent_snapshot_action
        else {
            unreachable!()
        };
        assert!(condition.is_none_or(|guard| selected(&table, guard, &choices)));
        let parent_inputs = table
            .owner_snapshot(parent_snapshot)
            .unwrap()
            .capture_inputs()
            .iter()
            .filter(|input| selected(&table, input.condition(), &choices))
            .collect::<Vec<_>>();
        let [parent_input] = parent_inputs.as_slice() else {
            panic!("parent snapshot must select its formed instance")
        };
        assert_eq!(parent_input.owner(), formed);
        replay_snapshot_choices(&table, parent_snapshot, &mut choices);
        let formed_instance = values.remove(&formed).unwrap();
        assert_eq!(formed_instance, parent_instance);
        assert!(values.insert(parent_snapshot, formed_instance).is_none());
        let second_entry = incomings
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
        let second_backedge = incomings
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
            .unwrap();
        let second_exhausted = incomings
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
            .unwrap();
        let [entry] = second_entry.bindings() else {
            panic!("outer has one phi root")
        };
        let [backedge] = second_backedge.bindings() else {
            panic!("outer has one phi root")
        };
        let [exhausted] = second_exhausted.bindings() else {
            panic!("outer has one phi root")
        };
        let parent_instance_node = graph
            .nodes()
            .iter()
            .position(|node| node.closure() == parent.closure())
            .unwrap();
        assert_eq!(entry.values()[0].source(), parent_snapshot);
        for binding in [entry, backedge, exhausted] {
            let [root] = binding.root_sources() else {
                panic!("the parent instance needs one root handle source")
            };
            assert_eq!(root.source(), binding.values()[0].source());
            assert_eq!(root.node(), parent_instance_node);
        }
        assert_eq!(selected_root(entry, &choices).source(), parent_snapshot);
        let (target, source, moved) = replay_instance_presence(second_entry, &values, &mut choices);
        assert_eq!(target, entry.target());
        assert_eq!(values.remove(&source), Some(moved));
        assert!(values.insert(entry.target(), moved).is_none());
        assert_eq!(backedge.target(), entry.target());
        assert_eq!(backedge.values()[0].source(), entry.target());
        assert_eq!(
            backedge.availability_selector(),
            header.availability_selector()
        );
        assert_eq!(backedge.available_when(), header.availability_condition());
        assert_eq!(
            backedge.values()[0].condition(),
            header.availability_condition()
        );
        for _ in 0..2 {
            let saved_before = saved_children.clone();
            assert_eq!(
                selected_root(backedge, &choices).source(),
                backedge.target()
            );
            let (target, source, old) =
                replay_instance_presence(second_backedge, &values, &mut choices);
            assert_eq!((target, source), (backedge.target(), backedge.target()));
            assert_eq!(values.insert(target, old), Some(old));
            assert_eq!(values[&entry.target()], parent_instance);
            assert_eq!(saved_children, saved_before);
            assert_eq!(saved_children[&(parent_instance, 0)], first_child);
            assert_eq!(saved_children[&(parent_instance, 1)], second_child);
        }
        assert_eq!(
            selected_root(exhausted, &choices).source(),
            backedge.target()
        );
        let (target, source, moved) =
            replay_instance_presence(second_exhausted, &values, &mut choices);
        assert_eq!(target, exhausted.target());
        assert_eq!(values.remove(&source), Some(moved));
        assert!(values.insert(exhausted.target(), moved).is_none());
        // Two live children have the same static lambda; release must start from the root
        // instance so neither child is conflated with the other one's capture layout.
        let root_drop = candidate
            .drops
            .iter()
            .find(|fact| {
                fact.owner() == Some(exhausted.target())
                    && matches!(fact.target(), DropTarget::Named(_))
            })
            .unwrap();
        assert_eq!(root_drop.target(), DropTarget::Named(outer_symbol));
        let outer_call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "outer()").then_some(id))
            .unwrap();
        assert_eq!(root_drop.point(), DropPoint::CallReturn(outer_call));
        assert_ne!(
            root_drop.condition(),
            Some(CleanupConditionId::NEVER),
            "the formed root must have a reachable release guard"
        );
        assert!(
            root_drop
                .condition()
                .is_none_or(|guard| selected(&table, guard, &choices))
        );
        let releases = steps
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(released_statement),
                    root,
                } if *point == root_drop.point() && released_statement.index() == statement => {
                    Some((*released_statement, *root))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(release_statement, release_root)] = releases.as_slice() else {
            panic!("the exit must release one formed root instance")
        };
        assert_eq!(*release_root, *root_drop);
        let release_graph = &plans[&release_statement.index()].capture_graph();
        assert_eq!(
            steps
                .iter()
                .filter(|(_, action)| match action {
                    IterationCleanupAction::Drop(fact)
                    | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => {
                        fact.owner() == Some(exhausted.target())
                    }
                    _ => false,
                })
                .count(),
            1,
            "the exit root must have one release action across all points"
        );
        assert!(!steps.iter().any(|(_, action)| {
            matches!(action, IterationCleanupAction::Drop(fact)
            if matches!(fact.target(), DropTarget::Captured { .. })
                && fact.instance_address().is_some_and(|address| {
                    table.instance_address(address)
                        .is_some_and(|address| address.root() == exhausted.target())
                }))
        }));
        let root_instance = values.remove(&release_root.owner().unwrap()).unwrap();
        assert_eq!(root_instance, parent_instance);
        enum ReleaseStep {
            Enter(usize),
            DropValue(usize, usize),
            Finish(usize),
        }
        #[derive(Debug, PartialEq, Eq)]
        enum Released {
            Value(usize),
            Environment(usize),
        }
        let mut pending = vec![ReleaseStep::Enter(root_instance)];
        let mut entered = BTreeSet::new();
        let mut released = Vec::new();
        while let Some(step) = pending.pop() {
            match step {
                ReleaseStep::Enter(instance) => {
                    assert!(
                        entered.insert(instance),
                        "an instance must be released once"
                    );
                    pending.push(ReleaseStep::Finish(instance));
                    for source in release_graph.nodes()[instance_nodes[&instance]].sources() {
                        assert_eq!(source.capture().mode(), ClosureCaptureMode::Owned);
                        assert_eq!(source.capture().effect(), ClosureCaptureEffect::Move);
                        if source.captured().is_empty() {
                            pending.push(ReleaseStep::DropValue(instance, source.position()));
                        } else {
                            let child = saved_children
                                .remove(&(instance, source.position()))
                                .unwrap();
                            assert!(source.captured().contains(&instance_nodes[&child]));
                            pending.push(ReleaseStep::Enter(child));
                        }
                    }
                }
                ReleaseStep::DropValue(instance, position) => released.push(Released::Value(
                    leaf_resources.remove(&(instance, position)).unwrap(),
                )),
                ReleaseStep::Finish(instance) => released.push(Released::Environment(instance)),
            }
        }
        assert!(saved_children.is_empty() && leaf_resources.is_empty());
        assert_eq!(
            released,
            [
                Released::Value(101),
                Released::Environment(second_child),
                Released::Value(102),
                Released::Environment(first_child),
                Released::Environment(root_instance),
            ]
        );
        let released_environments = released.iter().filter_map(|event| match event {
            Released::Environment(instance) => Some(instance),
            Released::Value(_) => None,
        });
        assert_eq!(
            dropped_old
                .iter()
                .chain(released_environments)
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            instance_nodes.len()
        );
        assert!(values.is_empty());
    }

    #[test]
    fn sibling_shared_capture_loans_keep_distinct_instance_paths() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "sibling-captures.ko",
                "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>, next: List<Int>) {
                    val xs = listOf(1)
                    var first: () -> Unit = {}
                    var second: () -> Unit = {}
                    for (_ in flags) {
                        second = first
                        { first = { read(xs) } }
                    }
                    var outer: move () -> Unit = move { val a = first()\nval b = second() }
                    for (_ in next) {}
                    val used = outer()
                }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.coexisting_capture_phi.is_some());
        let candidate = planner.into_candidate_facts();
        let plans = candidate
            .iterations
            .iter()
            .map(|plan| (plan.descriptor().statement().index(), plan))
            .collect::<BTreeMap<_, _>>();
        let steps = &candidate.cleanup_steps;
        let table = &candidate.cleanup_conditions;
        let outer_call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "outer()").then_some(id))
            .unwrap();
        let mut addresses = steps
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::EndCaptureLoan {
                    instance_address, ..
                } if *point == DropPoint::CallReturn(outer_call) => table
                    .instance_address(*instance_address)
                    .map(|address| (address.root(), address.capture_path().to_vec())),
                _ => None,
            })
            .collect::<Vec<_>>();
        addresses.sort_by(|left, right| left.1.cmp(&right.1));
        assert_eq!(addresses.len(), 2);
        assert_eq!(addresses[0].0, addresses[1].0);
        assert_eq!(addresses[0].1, [0]);
        assert_eq!(addresses[1].1, [1]);
        let mut captured_drops = candidate
            .drops
            .iter()
            .filter_map(|fact| match fact.target() {
                DropTarget::Captured { .. }
                    if fact.point() == DropPoint::CallReturn(outer_call) =>
                {
                    let address = table.instance_address(fact.instance_address()?)?;
                    let slot = table.capture_slot_value(fact.capture_slot()?)?;
                    Some((
                        address.root(),
                        address.capture_path().to_vec(),
                        slot.position(),
                    ))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        captured_drops.sort_by_key(|drop| drop.2);
        assert_eq!(captured_drops.len(), 2);
        assert_eq!(captured_drops[0], (addresses[0].0, Vec::new(), 0));
        assert_eq!(captured_drops[1], (addresses[0].0, Vec::new(), 1));

        let statement = *plans.keys().max().unwrap();
        let graph = &plans[&statement].capture_graph();
        let outer = graph
            .nodes()
            .iter()
            .find(|node| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                ) == Ok("move { val a = first()\nval b = second() }")
            })
            .unwrap();
        let inner = graph
            .nodes()
            .iter()
            .find(|node| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                ) == Ok("{ read(xs) }")
            })
            .unwrap();
        assert_eq!(outer.sources().len(), 2);
        assert!(outer.sources().iter().all(|source| {
            source
                .captured()
                .iter()
                .any(|&node| graph.nodes()[node].closure() == inner.closure())
        }));
        let symbol = |index: usize| match outer.sources()[index].capture().source() {
            ClosureCaptureSource::Symbol(symbol) => symbol,
            other => panic!("outer capture must read a named binding: {other:?}"),
        };
        let first = symbol(0);
        let second = symbol(1);
        let first_loop = *plans.keys().min().unwrap();
        let first_graph = &plans[&first_loop].capture_graph();
        let first_phis = &plans[&first_loop].closure_phis();
        let phi = |symbol, boundary| {
            first_phis
                .iter()
                .find(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
                .unwrap()
        };
        let first_header = phi(first, IterationPhiBoundary::Header);
        let second_header = phi(second, IterationPhiBoundary::Header);
        let first_exit = phi(first, IterationPhiBoundary::Exit);
        let second_exit = phi(second, IterationPhiBoundary::Exit);
        let first_incomings = &plans[&first_loop].closure_phi_incomings();
        let first_edge = |kind| {
            first_incomings
                .iter()
                .find(|incoming| incoming.kind() == kind)
                .unwrap()
        };
        let binding = |kind, target| {
            first_edge(kind)
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap()
        };
        let active_root = |binding: &IterationPhiIncomingBinding, choices: &BTreeMap<_, _>| {
            let roots = binding
                .root_sources()
                .iter()
                .filter(|root| selected(&table, root.condition(), choices))
                .collect::<Vec<_>>();
            let [root] = roots.as_slice() else {
                panic!("one formed closure root must reach each shared-loan phi binding")
            };
            **root
        };
        let first_entry = binding(IterationPhiIncomingKind::Entry, first_header.owner());
        let second_entry = binding(IterationPhiIncomingKind::Entry, second_header.owner());
        let first_back = binding(IterationPhiIncomingKind::Fallthrough, first_header.owner());
        let second_back = binding(IterationPhiIncomingKind::Fallthrough, second_header.owner());
        let first_out = binding(IterationPhiIncomingKind::Exhaustion, first_exit.owner());
        let second_out = binding(IterationPhiIncomingKind::Exhaustion, second_exit.owner());
        for incoming in [
            first_entry,
            second_entry,
            first_back,
            second_back,
            first_out,
            second_out,
        ] {
            assert_eq!(incoming.values().len(), 1);
        }
        let create_for = |wanted| {
            steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CreateClosureOwner { owner, .. }
                        if *owner == wanted =>
                    {
                        Some(*action)
                    }
                    _ => None,
                })
                .unwrap()
        };
        let mut next_instance = 0;
        let mut values = BTreeMap::new();
        let mut instance_closures = BTreeMap::new();
        let mut create = |action,
                          values: &mut BTreeMap<CleanupOwnerValueId, usize>,
                          instances: &mut BTreeMap<usize, ExpressionId>| {
            let IterationCleanupAction::CreateClosureOwner { owner, closure } = action else {
                unreachable!()
            };
            next_instance += 1;
            assert!(values.insert(owner, next_instance).is_none());
            assert!(instances.insert(next_instance, closure).is_none());
            next_instance
        };
        let initial_first = create(
            create_for(first_entry.values()[0].source()),
            &mut values,
            &mut instance_closures,
        );
        let initial_second = create(
            create_for(second_entry.values()[0].source()),
            &mut values,
            &mut instance_closures,
        );
        let ordinary_entries = first_edge(IterationPhiIncomingKind::Entry)
            .bindings()
            .iter()
            .filter(|binding| binding.root_sources().is_empty())
            .collect::<Vec<_>>();
        let [source_entry] = ordinary_entries.as_slice() else {
            panic!("the shared source needs one ordinary entry binding")
        };
        let [entry_value] = source_entry.values() else {
            panic!("shared source needs one entry value")
        };
        let Some(CleanupOwnerValue::Expression { expression, .. }) =
            table.owner_value(entry_value.source())
        else {
            panic!("shared source entry must be the list expression")
        };
        assert_eq!(
            sources.slice(parsed.ast().expressions().get(*expression).unwrap().span()),
            Ok("listOf(1)")
        );
        let mut source_values = BTreeMap::from([(entry_value.source(), 100_usize)]);
        let first_nodes = |instances: &BTreeMap<usize, ExpressionId>| {
            instances
                .iter()
                .filter_map(|(&instance, &closure)| {
                    first_graph
                        .nodes()
                        .iter()
                        .position(|node| node.closure() == closure)
                        .map(|node| (instance, node))
                })
                .collect::<BTreeMap<_, _>>()
        };
        let mut choices = BTreeMap::new();
        let entry_roots = [first_entry, second_entry]
            .into_iter()
            .map(|incoming| {
                let root = active_root(incoming, &choices);
                assert_eq!(root.source(), incoming.values()[0].source());
                assert_eq!(
                    first_graph.nodes()[root.node()].closure(),
                    instance_closures[&values[&root.source()]]
                );
                (incoming.target(), root.source())
            })
            .collect::<Vec<_>>();
        let mut edge_values = values.clone();
        edge_values.extend(
            source_values
                .iter()
                .map(|(&owner, &instance)| (owner, instance)),
        );
        let entry_transport = replay_captured_edge(
            &table,
            first_graph,
            first_phis,
            first_edge(IterationPhiIncomingKind::Entry),
            &edge_values,
            &first_nodes(&instance_closures),
            &BTreeMap::new(),
            &mut choices,
        );
        assert_eq!(
            entry_transport.len(),
            first_edge(IterationPhiIncomingKind::Entry).bindings().len()
        );
        for (target, source) in entry_roots {
            assert_eq!(entry_transport[&target], (source, values[&source]));
            let moved = values.remove(&source).unwrap();
            assert!(values.insert(target, moved).is_none());
        }
        let source_instance = source_values.remove(&entry_value.source()).unwrap();
        assert_eq!(
            entry_transport[&source_entry.target()],
            (entry_value.source(), source_instance)
        );
        assert!(
            source_values
                .insert(source_entry.target(), source_instance)
                .is_none()
        );
        let snapshot_for = |target| {
            steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CommitOwnerSnapshot {
                        owner,
                        target: actual,
                    } if *actual == target => Some(*owner),
                    _ => None,
                })
                .unwrap()
        };
        let first_snapshot = snapshot_for(first);
        let second_snapshot = snapshot_for(second);
        assert_eq!(
            table
                .owner_snapshot(second_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            first_header.owner()
        );
        let inner_create = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { closure, .. }
                    if *closure == inner.closure() =>
                {
                    Some(*action)
                }
                _ => None,
            })
            .unwrap();
        let IterationCleanupAction::CreateClosureOwner {
            owner: inner_owner, ..
        } = inner_create
        else {
            unreachable!()
        };
        assert_eq!(
            table
                .owner_snapshot(first_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            inner_owner
        );
        let inner_save = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == inner_owner => Some((*target, *input)),
                _ => None,
            })
            .unwrap();
        assert_eq!(inner_save.1.mode(), ClosureCaptureMode::Shared);
        let CleanupCaptureValue::Owner(source_owner) = inner_save.1.value() else {
            unreachable!()
        };
        let Some(CleanupOwnerValue::IterationPhi {
            statement: _,
            boundary: IterationPhiBoundary::Header,
            symbol: source_symbol,
            ..
        }) = table.owner_value(source_owner)
        else {
            panic!("shared source must be the first loop's header phi")
        };
        assert!(
            first_phis
                .iter()
                .any(|phi| phi.owner() == source_owner && phi.symbol() == *source_symbol)
        );
        assert_eq!(source_entry.target(), source_owner);
        let source_back = binding(IterationPhiIncomingKind::Fallthrough, source_owner);
        assert_eq!(source_back.values().len(), 1);
        assert_eq!(source_back.values()[0].source(), source_owner);
        let inner_slot = table.capture_slot_value(inner_save.0).unwrap();
        assert_eq!(inner_slot.environment(), inner_owner);
        assert_eq!(inner_slot.closure(), inner.closure());
        assert_eq!(inner_slot.source(), inner_save.1.source());
        assert_eq!(inner_slot.position(), 0);
        let mut source_slots = BTreeMap::new();
        let mut live_loans = BTreeMap::from([(source_values[&source_owner], 0_usize)]);
        let mut released_old = Vec::new();
        let old_second_drop = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::Drop(fact)
                    if fact.target() == DropTarget::Named(second)
                        && fact.owner() == Some(second_header.owner()) =>
                {
                    Some(*fact)
                }
                _ => None,
            })
            .unwrap();
        let snapshot_index = |wanted| {
            steps
                .iter()
                .position(|(_, action)| {
                    matches!(action,
                    IterationCleanupAction::SaveOwnerSnapshot { owner, .. } if *owner == wanted)
                })
                .unwrap()
        };
        let old_drop_index = steps
            .iter()
            .position(|(_, action)| {
                matches!(action, IterationCleanupAction::Drop(fact) if *fact == old_second_drop)
            })
            .unwrap();
        let early_ends = steps
            .iter()
            .enumerate()
            .filter_map(|(index, (point, action))| match action {
                IterationCleanupAction::EndCaptureLoan {
                    owner,
                    source,
                    condition,
                    ..
                } if *source == inner_save.1.source()
                    && *point != DropPoint::CallReturn(outer_call) =>
                {
                    assert_eq!(*owner, second_header.owner());
                    Some((index, *condition))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            early_ends.len(),
            2,
            "named/retained source paths are distinct"
        );
        let mut proof = table.clone();
        assert_eq!(
            proof.and(early_ends[0].1.unwrap(), early_ends[1].1.unwrap()),
            CleanupConditionId::NEVER,
            "one instance must not end the same loan on both source paths"
        );
        assert!(snapshot_index(second_snapshot) < old_drop_index);
        for (early_end_index, _) in &early_ends {
            assert!(old_drop_index < *early_end_index);
            assert!(*early_end_index < snapshot_index(first_snapshot));
        }
        for _ in 0..2 {
            let moved = values.remove(&first_header.owner()).unwrap();
            replay_snapshot_choices(&table, second_snapshot, &mut choices);
            assert!(values.insert(second_snapshot, moved).is_none());
            let old = values.remove(&old_second_drop.owner().unwrap()).unwrap();
            released_old.push(old);
            assert!(
                early_ends
                    .iter()
                    .all(|(_, guard)| !selected(&table, guard.unwrap(), &choices)),
                "the replaced second has no shared loan on either executed round"
            );
            assert!(
                steps.iter().all(|(point, action)| {
                    *point != old_second_drop.point()
                        || !matches!(action, IterationCleanupAction::Drop(fact)
                        if matches!(fact.target(), DropTarget::RetainedSource(_))
                            && selected(&table, fact.condition().unwrap(), &choices))
                }),
                "a live named source must not take the retained cleanup path"
            );
            let formed = create(inner_create, &mut values, &mut instance_closures);
            assert_eq!(instance_closures[&formed], inner.closure());
            assert_eq!(
                inner_save.1.value(),
                CleanupCaptureValue::Owner(source_owner)
            );
            let source = source_values[&source_owner];
            let position = table.capture_slot_value(inner_save.0).unwrap().position();
            assert!(source_slots.insert((formed, position), source).is_none());
            *live_loans.get_mut(&source).unwrap() += 1;
            let formed = values.remove(&inner_owner).unwrap();
            replay_snapshot_choices(&table, first_snapshot, &mut choices);
            assert!(values.insert(first_snapshot, formed).is_none());
            let writes = [first_back, second_back]
                .into_iter()
                .map(|incoming| {
                    let root = active_root(incoming, &choices);
                    assert_eq!(root.source(), incoming.values()[0].source());
                    assert_eq!(
                        first_graph.nodes()[root.node()].closure(),
                        instance_closures[&values[&root.source()]]
                    );
                    (incoming.target(), root.source(), values[&root.source()])
                })
                .collect::<Vec<_>>();
            let mut edge_values = values.clone();
            edge_values.extend(
                source_values
                    .iter()
                    .map(|(&owner, &instance)| (owner, instance)),
            );
            let back_transport = replay_captured_edge(
                &table,
                first_graph,
                first_phis,
                first_edge(IterationPhiIncomingKind::Fallthrough),
                &edge_values,
                &first_nodes(&instance_closures),
                &BTreeMap::new(),
                &mut choices,
            );
            assert_eq!(
                back_transport.len(),
                first_edge(IterationPhiIncomingKind::Fallthrough)
                    .bindings()
                    .len()
            );
            assert_eq!(
                back_transport[&source_owner],
                (source_owner, source_instance)
            );
            for (target, source, moved) in writes {
                assert_eq!(back_transport[&target], (source, moved));
                assert_eq!(values.remove(&source), Some(moved));
                assert!(values.insert(target, moved).is_none());
            }
        }
        assert_eq!(released_old, [initial_second, initial_first]);
        let newer = values[&first_header.owner()];
        let older = values[&second_header.owner()];
        assert_ne!(newer, older);
        let exit_roots = [first_out, second_out]
            .into_iter()
            .map(|incoming| {
                let root = active_root(incoming, &choices);
                assert_eq!(root.source(), incoming.values()[0].source());
                assert_eq!(
                    first_graph.nodes()[root.node()].closure(),
                    instance_closures[&values[&root.source()]]
                );
                (incoming.target(), root.source())
            })
            .collect::<Vec<_>>();
        let mut edge_values = values.clone();
        edge_values.extend(
            source_values
                .iter()
                .map(|(&owner, &instance)| (owner, instance)),
        );
        let before_exit = choices.clone();
        let exit_transport = replay_captured_edge(
            &table,
            first_graph,
            first_phis,
            first_edge(IterationPhiIncomingKind::Exhaustion),
            &edge_values,
            &first_nodes(&instance_closures),
            &BTreeMap::new(),
            &mut choices,
        );
        assert_eq!(exit_transport.len(), 2);
        for (target, source) in exit_roots {
            assert_eq!(exit_transport[&target], (source, values[&source]));
            let moved = values.remove(&source).unwrap();
            assert!(values.insert(target, moved).is_none());
        }
        let ordinary_exits = first_edge(IterationPhiIncomingKind::Exhaustion)
            .bindings()
            .iter()
            .filter(|binding| binding.root_sources().is_empty())
            .collect::<Vec<_>>();
        let [source_exit] = ordinary_exits.as_slice() else {
            panic!("shared source needs one ordinary exit binding")
        };
        assert!(source_exit.values().is_empty());
        assert!(!selected(
            &table,
            source_exit.available_when(),
            &before_exit
        ));
        assert_eq!(choices[&source_exit.availability_selector()], 0);
        assert!(!exit_transport.contains_key(&source_exit.target()));
        assert_eq!(source_values[&source_owner], source_instance);
        assert_eq!(values[&first_exit.owner()], newer);
        assert_eq!(values[&second_exit.owner()], older);
        let named_source_drops = steps
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::Drop(fact)
                    if fact.target() == DropTarget::Named(*source_symbol) =>
                {
                    Some((*point, *fact))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(named_source_drops.len(), 2);
        let (named_point, named_drop) = named_source_drops
            .iter()
            .find(|(_, fact)| fact.owner() == Some(source_owner))
            .unwrap();
        assert!(matches!(named_point,
            DropPoint::LoopExit(drop_statement) if drop_statement.index() == first_loop));
        assert!(
            named_drop
                .condition()
                .is_some_and(|guard| !selected(&table, guard, &choices))
        );
        let parent_create = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { closure, .. }
                    if *closure == outer.closure() =>
                {
                    Some(*action)
                }
                _ => None,
            })
            .unwrap();
        let IterationCleanupAction::CreateClosureOwner {
            owner: parent_owner,
            ..
        } = parent_create
        else {
            unreachable!()
        };
        let parent_instance = create(parent_create, &mut values, &mut instance_closures);
        let mut children = BTreeMap::new();
        for (_, action) in steps {
            let IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } = action
            else {
                continue;
            };
            if *owner != parent_owner {
                continue;
            }
            let CleanupCaptureValue::Owner(source) = input.value() else {
                unreachable!()
            };
            let slot = table.capture_slot_value(*target).unwrap();
            let position = slot.position();
            assert_eq!(slot.environment(), parent_owner);
            assert_eq!(slot.closure(), outer.closure());
            assert_eq!(slot.source(), input.source());
            assert_eq!(slot.source(), outer.sources()[position].capture().source());
            assert_eq!(
                (input.mode(), input.effect()),
                (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
            );
            let child = values.remove(&source).unwrap();
            assert!(children.insert(position, child).is_none());
        }
        assert_eq!(children, BTreeMap::from([(0, newer), (1, older)]));
        let parent_snapshot = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                    if *value == outer.closure() =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            table
                .owner_snapshot(parent_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            parent_owner
        );
        replay_snapshot_choices(&table, parent_snapshot, &mut choices);
        let moved = values.remove(&parent_owner).unwrap();
        assert_eq!(moved, parent_instance);
        assert!(values.insert(parent_snapshot, moved).is_none());
        let outer_symbol = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *owner == parent_snapshot =>
                {
                    Some(*target)
                }
                _ => None,
            })
            .unwrap();
        let outer_header = plans[&statement]
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == outer_symbol
            })
            .unwrap();
        let outer_exit = plans[&statement]
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == outer_symbol
            })
            .unwrap();
        assert_eq!(addresses[0].0, outer_exit.owner());
        let parent_aliases = BTreeSet::from([
            parent_owner,
            parent_snapshot,
            outer_header.owner(),
            outer_exit.owner(),
        ]);
        let mut parent_ends = steps
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::EndCaptureLoan {
                    source,
                    instance_address,
                    ..
                } if *source == inner_save.1.source() => {
                    let address = table.instance_address(*instance_address).unwrap();
                    parent_aliases
                        .contains(&address.root())
                        .then_some((*point, address.capture_path().to_vec()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        parent_ends.sort_by(|left, right| left.1.cmp(&right.1));
        assert_eq!(
            parent_ends,
            [
                (DropPoint::CallReturn(outer_call), vec![0]),
                (DropPoint::CallReturn(outer_call), vec![1]),
            ],
            "each saved child loan must end once at the parent call return"
        );
        let instance_nodes = instance_closures
            .iter()
            .filter_map(|(&instance, &closure)| {
                graph
                    .nodes()
                    .iter()
                    .position(|node| node.closure() == closure)
                    .map(|node| (instance, node))
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            instance_nodes[&parent_instance],
            graph
                .nodes()
                .iter()
                .position(|node| node.closure() == outer.closure())
                .unwrap()
        );
        assert_eq!(instance_nodes[&newer], instance_nodes[&older]);
        let captured = children
            .iter()
            .map(|(&position, &child)| ((parent_instance, position), child))
            .collect::<BTreeMap<_, _>>();
        let second_incomings = &plans[&statement].closure_phi_incomings();
        for kind in [
            IterationPhiIncomingKind::Entry,
            IterationPhiIncomingKind::Fallthrough,
            IterationPhiIncomingKind::Exhaustion,
        ] {
            let incoming = second_incomings
                .iter()
                .find(|incoming| incoming.kind() == kind)
                .unwrap();
            let target = if kind == IterationPhiIncomingKind::Exhaustion {
                outer_exit.owner()
            } else {
                outer_header.owner()
            };
            let binding = incoming
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap();
            assert_eq!(binding.values().len(), 1);
            let mut witnessed_children = BTreeSet::new();
            for origin in binding
                .origins()
                .iter()
                .filter(|origin| selected(&table, origin.condition(), &choices))
            {
                for environment in origin
                    .environments()
                    .iter()
                    .filter(|environment| selected(&table, environment.condition(), &choices))
                {
                    assert_eq!(values[&environment.instance_root()], parent_instance);
                    assert!(environment.capture_path().is_empty());
                    for capture in environment
                        .sources()
                        .iter()
                        .filter(|capture| selected(&table, capture.input().condition(), &choices))
                    {
                        assert!(capture.target().is_none());
                        assert!(capture.capture_slot().is_none());
                        assert!(capture.transport_value().is_none());
                        let (address, slot) = capture.transport_read().unwrap();
                        let address = table.instance_address(address).unwrap();
                        assert_eq!(address.root(), environment.instance_root());
                        assert!(address.capture_path().is_empty());
                        let position = table.capture_slot_value(slot).unwrap().position();
                        let child = children[&position];
                        assert_eq!(instance_closures[&child], inner.closure());
                        assert!(capture.captured().iter().any(|nested| {
                            selected(&table, nested.condition(), &choices)
                                && nested.environments().iter().any(|nested_environment| {
                                    selected(&table, nested_environment.condition(), &choices)
                                        && nested_environment.instance_root()
                                            == environment.instance_root()
                                        && nested_environment.capture_path() == [position]
                                })
                        }));
                        witnessed_children.insert(position);
                    }
                }
            }
            assert_eq!(witnessed_children, BTreeSet::from([0, 1]));
            let root_source = active_root(binding, &choices);
            assert_eq!(root_source.source(), binding.values()[0].source());
            assert_eq!(graph.nodes()[root_source.node()].closure(), outer.closure());
            assert_eq!(values[&root_source.source()], parent_instance);
            let before = choices.clone();
            let mut edge_values = values.clone();
            edge_values.extend(
                source_values
                    .iter()
                    .map(|(&owner, &instance)| (owner, instance)),
            );
            let transported = replay_captured_edge(
                &table,
                graph,
                &plans[&statement].closure_phis(),
                incoming,
                &edge_values,
                &instance_nodes,
                &captured,
                &mut choices,
            );
            assert_eq!(
                transported.len(),
                incoming
                    .bindings()
                    .iter()
                    .filter(|binding| { selected(&table, binding.available_when(), &before) })
                    .count()
            );
            assert_eq!(
                transported[&binding.target()],
                (root_source.source(), parent_instance)
            );
            for ordinary in incoming
                .bindings()
                .iter()
                .filter(|binding| binding.root_sources().is_empty())
            {
                if let Some(&(source, instance)) = transported.get(&ordinary.target()) {
                    assert_eq!(source_values[&source], instance);
                    if source != ordinary.target() {
                        assert_eq!(source_values.remove(&source), Some(instance));
                        assert!(source_values.insert(ordinary.target(), instance).is_none());
                    }
                } else {
                    assert!(!selected(&table, ordinary.available_when(), &before));
                    assert_eq!(choices[&ordinary.availability_selector()], 0);
                }
            }
            let old = values.remove(&root_source.source()).unwrap();
            assert_eq!(old, parent_instance);
            assert!(values.insert(binding.target(), old).is_none());
            let target_phi = if kind == IterationPhiIncomingKind::Exhaustion {
                outer_exit
            } else {
                outer_header
            };
            for root in target_phi.root_origins() {
                assert_eq!(
                    choices[&root.selector()],
                    usize::from(
                        root.node() == root_source.node() || root.node() == instance_nodes[&newer]
                    )
                );
            }
            assert_eq!(children, BTreeMap::from([(0, newer), (1, older)]));
        }
        assert_eq!(source_values.values().copied().collect::<Vec<_>>(), [100]);
        let (second_named_point, second_named_drop) = named_source_drops
            .iter()
            .find(|(_, fact)| fact.owner() != Some(source_owner))
            .unwrap();
        assert!(matches!(second_named_point,
            DropPoint::LoopExit(drop_statement) if drop_statement.index() == statement));
        assert!(matches!(
            table
                .owner_value(second_named_drop.owner().unwrap()),
            Some(CleanupOwnerValue::IterationPhi {
                statement: drop_statement,
                symbol,
                ..
            }) if drop_statement.index() == statement && *symbol == *source_symbol
        ));
        assert!(
            second_named_drop
                .condition()
                .is_some_and(|guard| !selected(&table, guard, &choices))
        );
        let root = values.remove(&outer_exit.owner()).unwrap();
        assert_eq!(root, parent_instance);
        assert_eq!(live_loans[&100], 2);

        // Replay only the actions selected by the formed snapshots and both phi loops.
        // The public plan remains deferred; this does not publish executable cleanup.
        let actions = steps
            .iter()
            .filter(|(point, _)| *point == DropPoint::CallReturn(outer_call))
            .map(|(_, action)| *action)
            .collect::<Vec<_>>();
        let mut releasing = BTreeMap::new();
        let mut ended = BTreeMap::new();
        let mut outcomes = BTreeMap::new();
        let mut test_results = Vec::new();
        let mut released_environments = Vec::new();
        let mut last_loan_guarded_drop_candidates = Vec::new();
        for action in actions {
            match action {
                IterationCleanupAction::Drop(fact)
                    if matches!(fact.target(), DropTarget::Captured { .. }) =>
                {
                    if fact
                        .condition()
                        .is_some_and(|guard| !selected(&table, guard, &choices))
                    {
                        continue;
                    }
                    let address = table
                        .instance_address(fact.instance_address().unwrap())
                        .unwrap();
                    assert_eq!(address.root(), outer_exit.owner());
                    assert!(address.capture_path().is_empty());
                    let position = table
                        .capture_slot_value(fact.capture_slot().unwrap())
                        .unwrap()
                        .position();
                    let child = children.remove(&position).unwrap();
                    assert!(releasing.insert(position, child).is_none());
                    released_environments.push(child);
                }
                IterationCleanupAction::EndCaptureLoan {
                    owner,
                    instance_address,
                    capture_slot: Some(slot),
                    condition,
                    source: captured_source,
                    value,
                    ..
                } => {
                    if condition.is_some_and(|guard| !selected(&table, guard, &choices)) {
                        continue;
                    }
                    assert_eq!(captured_source, inner_save.1.source());
                    let address = table.instance_address(instance_address).unwrap();
                    assert_eq!(address.root(), outer_exit.owner());
                    let [position] = address.capture_path() else {
                        panic!("loan must name one child instance")
                    };
                    let expected_source = outer.sources()[*position].capture().source();
                    assert!(matches!(
                        table.owner_value(owner),
                        Some(CleanupOwnerValue::IterationPhiSourceOwner {
                            environment,
                            closure,
                            source,
                            ..
                        }) if *environment == outer_exit.owner()
                            && *closure == outer.closure()
                            && *source == expected_source
                    ));
                    let child = releasing[position];
                    let capture_position = table.capture_slot_value(slot).unwrap().position();
                    assert_eq!(capture_position, 0);
                    let layout = table.capture_slot_value(slot).unwrap();
                    assert_eq!(layout.closure(), inner.closure());
                    assert_eq!(layout.source(), inner_save.1.source());
                    let CleanupCaptureValue::Owner(static_source) = value else {
                        panic!("tracked source must have a phi owner")
                    };
                    assert!(matches!(
                        table.owner_value(static_source),
                        Some(CleanupOwnerValue::IterationPhiSourceOwner { environment, .. })
                            if *environment == outer_exit.owner()
                    ));
                    let source = source_slots[&(child, capture_position)];
                    assert_eq!(source, 100);
                    *live_loans.get_mut(&source).unwrap() -= 1;
                    assert!(
                        ended
                            .insert((instance_address, slot), (condition, static_source))
                            .is_none()
                    );
                }
                IterationCleanupAction::TestLastCaptureLoan {
                    owner,
                    instance_address,
                    capture_slot,
                    selector,
                    condition,
                    ..
                } => {
                    if condition.is_some_and(|guard| !selected(&table, guard, &choices)) {
                        continue;
                    }
                    let address = table.instance_address(instance_address).unwrap();
                    assert_eq!(address.root(), outer_exit.owner());
                    let [position] = address.capture_path() else {
                        panic!("test must name one child instance")
                    };
                    let child = releasing[position];
                    let layout = table.capture_slot_value(capture_slot).unwrap();
                    assert_eq!(layout.closure(), inner.closure());
                    assert_eq!(layout.source(), inner_save.1.source());
                    let source = source_slots[&(child, layout.position())];
                    assert_eq!(
                        ended.get(&(instance_address, capture_slot)),
                        Some(&(condition, owner))
                    );
                    let last = live_loans[&source] == 0;
                    assert!(choices.insert(selector, usize::from(last)).is_none());
                    test_results.push(last);
                    assert!(
                        outcomes
                            .insert((instance_address, capture_slot), (selector, last, source))
                            .is_none()
                    );
                }
                IterationCleanupAction::Drop(fact)
                    if matches!(fact.target(), DropTarget::RetainedSource(_)) =>
                {
                    let address = fact.instance_address().unwrap();
                    let slot = fact.capture_slot().unwrap();
                    assert_eq!(
                        fact.target(),
                        DropTarget::RetainedSource(inner_save.1.source())
                    );
                    assert_eq!(fact.owner(), Some(ended[&(address, slot)].1));
                    let (selector, last, source) = outcomes[&(address, slot)];
                    // Other source selectors are not evaluated by this deferred test;
                    // prove only that the candidate requires its own last-loan true arm.
                    fn possible(
                        table: &CleanupConditions,
                        condition: CleanupConditionId,
                        wanted: crate::ownership_checking::CleanupSelectorId,
                        arm: usize,
                    ) -> bool {
                        match table.get(condition).unwrap() {
                            CleanupCondition::Always => true,
                            CleanupCondition::Never => false,
                            CleanupCondition::Choice { selector, branches }
                                if *selector == wanted =>
                            {
                                possible(table, branches[arm], wanted, arm)
                            }
                            CleanupCondition::Choice { branches, .. } => branches
                                .iter()
                                .any(|branch| possible(table, *branch, wanted, arm)),
                        }
                    }
                    let guard = fact.condition().unwrap();
                    assert!(!possible(&table, guard, selector, 0));
                    assert!(possible(&table, guard, selector, 1));
                    assert_eq!(selected(&table, guard, &choices), last);
                    if selected(&table, guard, &choices) {
                        last_loan_guarded_drop_candidates.push(source);
                    }
                }
                IterationCleanupAction::Drop(fact)
                    if matches!(fact.target(), DropTarget::Named(_)) =>
                {
                    if fact
                        .condition()
                        .is_some_and(|guard| !selected(&table, guard, &choices))
                    {
                        continue;
                    }
                    assert_eq!(fact.owner(), Some(outer_exit.owner()));
                    assert!(children.is_empty());
                    released_environments.push(root);
                }
                _ => {}
            }
        }
        assert_eq!(released_environments, [older, newer, root]);
        assert_eq!(test_results, [false, true]);
        assert_eq!(live_loans[&100], 0);
        assert_eq!(last_loan_guarded_drop_candidates, [100]);
        let retained_source_drops = steps
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::Drop(fact)
                    if fact.target() == DropTarget::RetainedSource(inner_save.1.source())
                        && *point == DropPoint::CallReturn(outer_call) =>
                {
                    Some((*point, *fact))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(retained_source_drops.len(), 2);
        assert!(!steps.iter().any(|(_, action)| matches!(action,
            IterationCleanupAction::Drop(fact)
                if matches!(fact.target(),
                    DropTarget::Temporary(value) if value == *expression)
                    || matches!(fact.target(),
                        DropTarget::Captured { source, .. } if source == inner_save.1.source()))));
        assert_eq!(
            retained_source_drops
                .iter()
                .filter(|(_, fact)| fact
                    .condition()
                    .is_some_and(|guard| selected(&table, guard, &choices)))
                .count(),
            1
        );
        assert!(source_slots.len() == 2 && releasing.len() == 2);
        assert_eq!(
            released_old
                .iter()
                .chain(&released_environments)
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            instance_closures.len()
        );
        assert!(values.is_empty());
    }

    #[test]
    fn untracked_copyable_shared_capture_keeps_loan_ends() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "copyable-sibling-loans.ko",
                "fun read(n: Int) {}\nfun run(flags: List<Int>, next: List<Int>) {
                    val n = 1
                    var first: () -> Unit = {}
                    var second: () -> Unit = {}
                    for (_ in flags) {
                        second = first
                        { first = { read(n) } }
                    }
                    var outer: move () -> Unit = move { val a = first()\nval b = second() }
                    for (_ in next) {}
                    val used = outer()
                }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        let outer_call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "outer()").then_some(id))
            .unwrap();
        let actions = planner
            .cleanup
            .iter()
            .filter(|(point, _)| *point == DropPoint::CallReturn(outer_call))
            .map(|(_, action)| action)
            .collect::<Vec<_>>();
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, IterationCleanupAction::EndCaptureLoan { .. }))
                .count(),
            2,
            "each formed sibling must end its own shared capture loan"
        );
        assert!(!actions.iter().any(|action| matches!(
            action,
            IterationCleanupAction::ReleaseClosureInstances { .. }
        )));
    }

    #[test]
    fn conditional_diamond_keeps_distinct_saved_paths_at_exhaustion() {
        fn paths_to(
            table: &CleanupConditions,
            origin: &IterationPhiIncomingOrigin,
            node: usize,
            capture_position: usize,
            paths: &mut BTreeSet<Vec<usize>>,
            conditions: &mut Vec<(
                Vec<usize>,
                CleanupConditionId,
                CleanupOwnerValueId,
                CleanupCaptureValue,
                CleanupOwnerValueId,
            )>,
            reads: &mut usize,
        ) {
            for environment in origin.environments() {
                for source in environment.sources() {
                    if let Some((address, _)) = source.transport_read() {
                        let address = table.instance_address(address).unwrap();
                        assert_eq!(address.root(), environment.instance_root());
                        assert_eq!(address.capture_path(), environment.capture_path());
                        *reads += 1;
                    }
                }
            }
            if origin.node() == node {
                for environment in origin.environments() {
                    let path = environment.capture_path().to_vec();
                    paths.insert(path.clone());
                    let leaf_reads = environment
                        .sources()
                        .iter()
                        .filter_map(IterationPhiIncomingSource::transport_read)
                        .filter(|(_, slot)| {
                            table.capture_slot_value(*slot).unwrap().position() == capture_position
                        })
                        .count();
                    assert_eq!(leaf_reads, 1, "base must read its own captured source");
                    assert_eq!(environment.sources().len(), 1);
                    conditions.push((
                        path,
                        environment.condition(),
                        environment.instance_root(),
                        environment.sources()[0].input().value(),
                        environment.owner(),
                    ));
                }
            }
            for nested in origin
                .environments()
                .iter()
                .flat_map(|environment| environment.sources())
                .flat_map(|source| source.captured())
            {
                paths_to(
                    table,
                    nested,
                    node,
                    capture_position,
                    paths,
                    conditions,
                    reads,
                );
            }
        }

        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "conditional-diamond.ko",
                r#"fun read(xs: List<Int>) {}
fun run(own xs: List<Int>, flag: Boolean, flags: List<Int>) {
    var base: move () -> Unit = move { read(xs) }
    var f: move () -> Unit = move {}
    var g: move () -> Unit = move {}
    if (flag) { f = move { base() } } else { g = move { base() } }
    var outer: move () -> Unit = move { val first = f()
val second = g() }
    for (_ in flags) {}
    val used = outer()
}"#,
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.coexisting_capture_phi.is_some());
        let candidate = planner.into_candidate_facts();
        let plan = candidate
            .iterations
            .iter()
            .find(|plan| plan.descriptor().statement() == statement)
            .unwrap();
        let table = &candidate.cleanup_conditions;
        let graph = plan.capture_graph();
        let incomings = plan.closure_phi_incomings();
        let base = graph
            .nodes()
            .iter()
            .position(|node| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                ) == Ok("move { read(xs) }")
            })
            .unwrap();
        assert_eq!(graph.nodes()[base].sources().len(), 1);
        let base_capture_position = graph.nodes()[base].sources()[0].position();
        let outer = graph
            .nodes()
            .iter()
            .position(|node| {
                sources
                    .slice(
                        parsed
                            .ast()
                            .expressions()
                            .get(node.closure())
                            .unwrap()
                            .span(),
                    )
                    .unwrap()
                    .starts_with("move { val first = f()")
            })
            .unwrap();
        let phis = plan.closure_phis();
        let mut path_conditions_by_edge = [Vec::new(), Vec::new()];
        for (index, kind) in [
            IterationPhiIncomingKind::Entry,
            IterationPhiIncomingKind::Exhaustion,
        ]
        .into_iter()
        .enumerate()
        {
            let edge = incomings.iter().find(|edge| edge.kind() == kind).unwrap();
            let boundary = if kind == IterationPhiIncomingKind::Entry {
                IterationPhiBoundary::Header
            } else {
                IterationPhiBoundary::Exit
            };
            let phi = phis
                .iter()
                .find(|phi| phi.boundary() == boundary && phi.root_nodes().contains(&outer))
                .unwrap();
            let binding = edge
                .bindings()
                .iter()
                .find(|binding| binding.target() == phi.owner())
                .unwrap();
            assert_eq!(
                binding.presence_source(),
                IterationPhiPresenceSource::CapturedInstances,
                "this graph can carry a conditional child instance across the loop"
            );
            let mut paths = BTreeSet::new();
            let mut path_conditions = Vec::new();
            let mut reads = 0;
            for origin in binding.origins() {
                paths_to(
                    table,
                    origin,
                    base,
                    base_capture_position,
                    &mut paths,
                    &mut path_conditions,
                    &mut reads,
                );
            }
            assert_eq!(paths, BTreeSet::from([vec![0, 0], vec![1, 0]]));
            assert!(reads > 0);
            path_conditions_by_edge[index] = path_conditions;
            if kind == IterationPhiIncomingKind::Exhaustion {
                assert!(binding.capture_slots_to_clear().is_empty());
            }
        }
        let outer_closure = graph.nodes()[outer].closure();
        let (outer_snapshot_index, outer_owner) = candidate
            .cleanup_steps
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                    if *value == outer_closure =>
                {
                    Some((index, *owner))
                }
                _ => None,
            })
            .unwrap();
        let snapshot = table.owner_snapshot(outer_owner).unwrap();
        assert_eq!(snapshot.copies().len(), 3);
        let control = snapshot.copies()[0].source();
        assert!(matches!(
            table.selector(control).unwrap().source(),
            CleanupSelectorSource::Control(_)
        ));
        let branch_sources = snapshot.copies()[1..]
            .iter()
            .map(|copy| copy.source())
            .collect::<BTreeSet<_>>();
        let header = phis
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Header && phi.root_nodes().contains(&outer)
            })
            .unwrap()
            .owner();
        let exit = phis
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.root_nodes().contains(&outer)
            })
            .unwrap()
            .owner();
        for (branch, expected) in [(0, vec![0, 0]), (1, vec![1, 0])] {
            let mut choices = BTreeMap::from([(control, branch)]);
            let mut choices_before_cleanup = BTreeMap::new();
            for (index, (_, action)) in candidate
                .cleanup_steps
                .iter()
                .take(outer_snapshot_index + 1)
                .enumerate()
            {
                choices_before_cleanup.insert(index, choices.clone());
                if let IterationCleanupAction::SaveOwnerSnapshot {
                    condition, owner, ..
                } = action
                {
                    let snapshot = table.owner_snapshot(*owner).unwrap();
                    if (*owner == outer_owner
                        || snapshot
                            .copies()
                            .iter()
                            .any(|copy| branch_sources.contains(&copy.target())))
                        && condition.is_none_or(|guard| selected(table, guard, &choices))
                    {
                        replay_snapshot_choices(table, *owner, &mut choices);
                    }
                }
            }
            let branch_snapshots = candidate
                .cleanup_steps
                .iter()
                .enumerate()
                .filter_map(|(index, (_, action))| match action {
                    IterationCleanupAction::SaveOwnerSnapshot {
                        condition: Some(guard),
                        owner,
                        ..
                    } if index < outer_snapshot_index
                        && selected(table, *guard, &choices_before_cleanup[&index]) =>
                    {
                        Some(*owner)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [branch_snapshot] = branch_snapshots.as_slice() else {
                panic!("one branch must form the captured inner environment")
            };
            let [branch_input] = table
                .owner_snapshot(*branch_snapshot)
                .unwrap()
                .capture_inputs()
            else {
                panic!("branch snapshot must retain one formed wrapper")
            };
            let wrapper_owner = branch_input.owner();
            let wrapper_saves = candidate
                .cleanup_steps
                .iter()
                .enumerate()
                .filter_map(|(index, (_, action))| match action {
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if *owner == wrapper_owner
                        && index < outer_snapshot_index
                        && selected(table, input.condition(), &choices_before_cleanup[&index]) =>
                    {
                        Some((*target, *input))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(wrapper_target, wrapper_input)] = wrapper_saves.as_slice() else {
                panic!("the selected wrapper must capture the formed base")
            };
            let CleanupCaptureValue::Owner(base_owner) = wrapper_input.value() else {
                panic!("the base is an owned closure value")
            };
            let [outer_input] = snapshot.capture_inputs() else {
                panic!("outer snapshot must retain one formed parent")
            };
            let outer_formed = outer_input.owner();
            let outer_saves = candidate
                .cleanup_steps
                .iter()
                .enumerate()
                .filter_map(|(index, (_, action))| match action {
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if *owner == outer_formed
                        && index < outer_snapshot_index
                        && selected(table, input.condition(), &choices_before_cleanup[&index]) =>
                    {
                        Some((*target, *input))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(outer_saves.len(), 2);
            let other_owner = outer_saves
                .iter()
                .find_map(|(_, input)| match input.value() {
                    CleanupCaptureValue::Owner(owner) if owner != *branch_snapshot => Some(owner),
                    _ => None,
                })
                .unwrap();
            let formed_closure = |owner| {
                candidate
                    .cleanup_steps
                    .iter()
                    .find_map(|(_, action)| match action {
                        IterationCleanupAction::CreateClosureOwner {
                            owner: formed,
                            closure,
                        } if *formed == owner => Some(*closure),
                        _ => None,
                    })
                    .unwrap()
            };
            assert_eq!(formed_closure(base_owner), graph.nodes()[base].closure());
            assert_eq!(formed_closure(outer_formed), outer_closure);
            let assert_formed_before_snapshot =
                |formed_owner, snapshot_owner, saves| {
                    let closure = formed_closure(formed_owner);
                    let creates = candidate
                        .cleanup_steps
                        .iter()
                        .enumerate()
                        .filter_map(|(index, (point, action))| match action {
                            IterationCleanupAction::CreateClosureOwner {
                                owner,
                                closure: value,
                            } if *owner == formed_owner => Some((index, *point, *value)),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    let [(create_index, create_point, created_closure)] = creates.as_slice() else {
                        panic!("one CreateClosureOwner must form the selected environment")
                    };
                    assert_eq!(*created_closure, closure);
                    assert_eq!(*create_point, DropPoint::AfterExpression(closure));
                    let snapshots =
                        candidate
                            .cleanup_steps
                            .iter()
                            .enumerate()
                            .filter_map(|(index, (point, action))| match action {
                                IterationCleanupAction::SaveOwnerSnapshot {
                                    owner, value, ..
                                } if *owner == snapshot_owner => Some((index, *point, *value)),
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                    let [(snapshot_index, snapshot_point, snapshot_value)] = snapshots.as_slice()
                    else {
                        panic!("one snapshot must retain the selected environment")
                    };
                    assert_eq!(*snapshot_value, closure);
                    assert_eq!(*snapshot_point, DropPoint::AfterExpression(closure));
                    assert!(create_index < snapshot_index);
                    let save_steps = candidate.cleanup_steps
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (point, action))| {
                        matches!(action, IterationCleanupAction::SaveClosureCapture { owner, .. }
                            if *owner == formed_owner)
                        .then_some((index, *point))
                    })
                    .collect::<Vec<_>>();
                    assert_eq!(save_steps.len(), saves);
                    assert!(save_steps.iter().all(|(index, point)| {
                        *point == DropPoint::AfterExpression(closure)
                            && create_index < index
                            && index < snapshot_index
                    }));
                    *snapshot_index
                };
            assert!(
                assert_formed_before_snapshot(wrapper_owner, *branch_snapshot, 1)
                    < outer_snapshot_index
            );
            assert_eq!(
                assert_formed_before_snapshot(outer_formed, outer_owner, 4),
                outer_snapshot_index
            );
            let instance_closures = BTreeMap::from([
                (1, formed_closure(base_owner)),
                (2, formed_closure(wrapper_owner)),
                (3, formed_closure(other_owner)),
                (4, formed_closure(outer_formed)),
            ]);
            let mut formed_values = BTreeMap::from([
                (base_owner, 1),
                (wrapper_owner, 2),
                (other_owner, 3),
                (outer_formed, 4),
            ]);
            assert_eq!(formed_values.len(), 4);
            let mut saved_children = BTreeMap::new();
            let wrapper_slot = table.capture_slot_value(*wrapper_target).unwrap();
            assert_eq!(wrapper_slot.environment(), wrapper_owner);
            assert_eq!(wrapper_slot.closure(), instance_closures[&2]);
            assert_eq!(wrapper_slot.source(), wrapper_input.source());
            let base_instance = formed_values.remove(&base_owner).unwrap();
            assert!(
                saved_children
                    .insert((2, wrapper_slot.position()), base_instance)
                    .is_none()
            );
            let wrapper_instance = formed_values.remove(&wrapper_owner).unwrap();
            assert!(
                formed_values
                    .insert(*branch_snapshot, wrapper_instance)
                    .is_none()
            );
            for (target, input) in outer_saves {
                let slot = table.capture_slot_value(target).unwrap();
                assert_eq!(slot.environment(), outer_formed);
                assert_eq!(slot.closure(), outer_closure);
                assert_eq!(slot.source(), input.source());
                let CleanupCaptureValue::Owner(source) = input.value() else {
                    panic!("outer captures formed closure values")
                };
                let child = formed_values.remove(&source).unwrap();
                assert!(saved_children.insert((4, slot.position()), child).is_none());
            }
            let parent_instance = formed_values.remove(&outer_formed).unwrap();
            assert!(formed_values.insert(outer_owner, parent_instance).is_none());
            assert_eq!(formed_values.len(), 1);
            let path_to_base = |root| {
                let paths = graph.nodes()[outer]
                    .sources()
                    .iter()
                    .flat_map(|source| {
                        let child = saved_children[&(root, source.position())];
                        let child_node = graph
                            .nodes()
                            .iter()
                            .position(|node| node.closure() == instance_closures[&child])
                            .unwrap();
                        assert!(source.captured().contains(&child_node));
                        graph.nodes()[child_node]
                            .sources()
                            .iter()
                            .filter_map(|nested| {
                                let leaf = saved_children.get(&(child, nested.position()))?;
                                assert_eq!(*leaf, base_instance);
                                assert_eq!(instance_closures[leaf], graph.nodes()[base].closure());
                                assert!(nested.captured().contains(&base));
                                Some(vec![source.position(), nested.position()])
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let [path] = paths.as_slice() else {
                    panic!("only one formed parent capture contains the base instance")
                };
                path.clone()
            };
            let formed_path = path_to_base(parent_instance);
            assert_eq!(formed_path, expected);
            let replay_instance_edge = |edge: &crate::ownership_checking::IterationPhiIncoming,
                                        target: CleanupOwnerValueId,
                                        values: &BTreeMap<CleanupOwnerValueId, usize>,
                                        choices: &mut BTreeMap<
                crate::ownership_checking::CleanupSelectorId,
                usize,
            >| {
                let before = choices.clone();
                assert!(selected(table, edge.condition(), &before));
                let binding = edge
                    .bindings()
                    .iter()
                    .find(|binding| binding.target() == target)
                    .unwrap();
                assert_eq!(
                    binding.presence_source(),
                    IterationPhiPresenceSource::CapturedInstances
                );
                let roots = binding
                    .root_sources()
                    .iter()
                    .filter(|root| selected(table, root.condition(), &before))
                    .collect::<Vec<_>>();
                let [root] = roots.as_slice() else {
                    panic!("one formed parent reaches this phi edge")
                };
                let selected_values = binding
                    .values()
                    .iter()
                    .filter(|value| selected(table, value.condition(), &before))
                    .collect::<Vec<_>>();
                let [value] = selected_values.as_slice() else {
                    panic!("one available owner value reaches this phi edge")
                };
                assert_eq!(value.source(), root.source());
                assert!(selected(table, binding.available_when(), &before));
                let root_instance = values[&root.source()];
                assert_eq!(root_instance, parent_instance);
                let mut pending = vec![root_instance];
                let mut visited = BTreeSet::new();
                let mut reached = BTreeSet::new();
                while let Some(instance) = pending.pop() {
                    assert!(
                        visited.insert(instance),
                        "formed owned instance visited twice"
                    );
                    let node = graph
                        .nodes()
                        .iter()
                        .position(|node| node.closure() == instance_closures[&instance])
                        .unwrap();
                    reached.insert(node);
                    for source in graph.nodes()[node].sources() {
                        if let Some(&child) = saved_children.get(&(instance, source.position())) {
                            let child_node = graph
                                .nodes()
                                .iter()
                                .position(|node| node.closure() == instance_closures[&child])
                                .unwrap();
                            assert!(source.captured().contains(&child_node));
                            pending.push(child);
                        }
                    }
                }
                assert_eq!(visited.len(), 4);
                assert!(reached.contains(&base));
                assert!(reached.contains(&outer));
                let layout = phis.iter().find(|phi| phi.owner() == target).unwrap();
                let expected_writes = layout
                    .origins()
                    .iter()
                    .map(|origin| (origin.node(), origin.selector()))
                    .collect::<BTreeMap<_, _>>();
                let actual_writes = binding
                    .selector_writes()
                    .iter()
                    .map(|write| (write.node(), write.target()))
                    .collect::<BTreeMap<_, _>>();
                assert_eq!(binding.selector_writes().len(), expected_writes.len());
                assert_eq!(actual_writes, expected_writes);
                let mut writes = vec![(
                    binding.availability_selector(),
                    usize::from(selected(table, binding.available_when(), &before)),
                )];
                writes.extend(
                    binding.selector_writes().iter().map(|write| {
                        (write.target(), usize::from(reached.contains(&write.node())))
                    }),
                );
                for (selector, value) in writes {
                    choices.insert(selector, value);
                }
                for write in binding.selector_writes() {
                    assert_eq!(
                        choices[&write.target()],
                        usize::from(reached.contains(&write.node()))
                    );
                }
                (value.source(), root_instance)
            };
            let root_drop = candidate
                .drops
                .iter()
                .find(|fact| {
                    fact.owner() == Some(exit) && matches!(fact.target(), DropTarget::Named(_))
                })
                .unwrap();
            let parent_aliases = [outer_formed, outer_owner, header, exit];
            let assert_no_early_parent_cleanup =
                |choices: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>| {
                    for (point, action) in &candidate.cleanup_steps {
                        if *point == root_drop.point() {
                            continue;
                        }
                        let fact = match action {
                            IterationCleanupAction::Drop(fact)
                            | IterationCleanupAction::ReleaseClosureInstances {
                                root: fact, ..
                            } => fact,
                            _ => continue,
                        };
                        let reaches_parent = fact.target() == root_drop.target()
                            || fact
                                .owner()
                                .is_some_and(|owner| parent_aliases.contains(&owner))
                            || fact.instance_address().is_some_and(|address| {
                                parent_aliases
                                    .contains(&table.instance_address(address).unwrap().root())
                            });
                        assert!(
                            !reaches_parent
                                || fact
                                    .condition()
                                    .is_some_and(|guard| !selected(table, guard, choices)),
                            "parent instance released before its call: {point:?} {action:?}"
                        );
                    }
                };
            for rounds in [0, 2] {
                let mut choices = choices.clone();
                assert_no_early_parent_cleanup(&choices);
                let mut values = BTreeMap::new();
                let entry = incomings
                    .iter()
                    .find(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
                    .unwrap();
                let entry_binding = entry
                    .bindings()
                    .iter()
                    .find(|binding| binding.target() == header)
                    .unwrap();
                assert_eq!(entry_binding.values().len(), 1);
                assert_eq!(entry_binding.values()[0].source(), outer_owner);
                assert!(
                    values
                        .insert(entry_binding.values()[0].source(), parent_instance)
                        .is_none()
                );
                for (index, kind) in [
                    IterationPhiIncomingKind::Entry,
                    IterationPhiIncomingKind::Exhaustion,
                ]
                .into_iter()
                .enumerate()
                {
                    if rounds > 0 && kind == IterationPhiIncomingKind::Exhaustion {
                        let fallthrough = incomings
                            .iter()
                            .find(|edge| edge.kind() == IterationPhiIncomingKind::Fallthrough)
                            .unwrap();
                        for _ in 0..rounds {
                            let (source, incoming_instance) =
                                replay_instance_edge(fallthrough, header, &values, &mut choices);
                            let binding = fallthrough
                                .bindings()
                                .iter()
                                .find(|binding| binding.target() == header)
                                .unwrap();
                            assert_eq!(binding.values().len(), 1);
                            assert_eq!(binding.values()[0].source(), source);
                            assert_eq!(source, header);
                            assert_eq!(values.remove(&source), Some(incoming_instance));
                            assert!(values.insert(header, incoming_instance).is_none());
                            assert_no_early_parent_cleanup(&choices);
                        }
                    }
                    let edge = incomings.iter().find(|edge| edge.kind() == kind).unwrap();
                    let target = if kind == IterationPhiIncomingKind::Entry {
                        header
                    } else {
                        exit
                    };
                    let (source, incoming_instance) =
                        replay_instance_edge(edge, target, &values, &mut choices);
                    let binding = edge
                        .bindings()
                        .iter()
                        .find(|binding| binding.target() == target)
                        .unwrap();
                    assert_eq!(binding.values().len(), 1);
                    assert_eq!(binding.values()[0].source(), source);
                    if kind == IterationPhiIncomingKind::Exhaustion {
                        assert_eq!(source, header);
                    }
                    assert_eq!(values.remove(&source), Some(incoming_instance));
                    assert!(values.insert(target, incoming_instance).is_none());
                    assert_eq!(path_to_base(values[&target]), formed_path);
                    assert_no_early_parent_cleanup(&choices);
                    let selected_paths = path_conditions_by_edge[index]
                        .iter()
                        .filter(|(_, condition, _, _, _)| selected(table, *condition, &choices))
                        .map(|(path, _, root, _, _)| {
                            assert_eq!(*root, source);
                            assert_eq!(values[&target], parent_instance);
                            path.clone()
                        })
                        .collect::<BTreeSet<_>>();
                    if kind == IterationPhiIncomingKind::Entry {
                        assert_eq!(
                            selected_paths,
                            BTreeSet::from([formed_path.clone()]),
                            "{kind:?}, branch {branch}, rounds {rounds}"
                        );
                    } else {
                        // 静态 header presence 无法判定实际父实例的哪条捕获边存在；
                        // Exhaustion 的候选可以过近似，公开计划仍须 deferred。
                        assert!(
                            selected_paths.contains(&formed_path),
                            "{kind:?}, branch {branch}, rounds {rounds}"
                        );
                    }
                }
                let (base_value, _base_owner) = path_conditions_by_edge[1]
                    .iter()
                    .find(|(path, condition, _, _, _)| {
                        path == &formed_path && selected(table, *condition, &choices)
                    })
                    .map(|(_, _, _, value, owner)| (*value, *owner))
                    .unwrap();
                let selected_drops = candidate
                    .cleanup_steps
                    .iter()
                    .filter(|(point, _)| *point == root_drop.point())
                    .filter_map(|(_, action)| match action {
                        IterationCleanupAction::Drop(fact)
                            if fact
                                .condition()
                                .is_none_or(|guard| selected(table, guard, &choices)) =>
                        {
                            Some(*fact)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let selected_root_cleanup = candidate
                    .cleanup_steps
                    .iter()
                    .filter_map(|(point, action)| {
                        let fact = match action {
                            IterationCleanupAction::Drop(fact)
                            | IterationCleanupAction::ReleaseClosureInstances {
                                root: fact, ..
                            } => fact,
                            _ => return None,
                        };
                        let from_exit = fact.owner() == Some(exit)
                            || fact.instance_address().is_some_and(|address| {
                                table.instance_address(address).unwrap().root() == exit
                            });
                        (from_exit
                            && fact
                                .condition()
                                .is_none_or(|guard| selected(table, guard, &choices)))
                        .then_some((*point, *action))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    selected_root_cleanup,
                    selected_drops
                        .iter()
                        .map(|fact| (root_drop.point(), IterationCleanupAction::Drop(*fact)))
                        .collect::<Vec<_>>()
                );
                assert_eq!(selected_drops.last(), Some(root_drop));
                let captured = selected_drops[..selected_drops.len() - 1]
                    .iter()
                    .map(|fact| {
                        let DropTarget::Captured {
                            owner: _,
                            closure,
                            source,
                            value,
                            ..
                        } = fact.target()
                        else {
                            panic!("only the root drop may be named")
                        };
                        let address = table
                            .instance_address(fact.instance_address().unwrap())
                            .unwrap();
                        assert_eq!(address.root(), exit);
                        let slot = table
                            .capture_slot_value(fact.capture_slot().unwrap())
                            .unwrap();
                        assert_eq!(slot.closure(), closure);
                        assert_eq!(slot.source(), source);
                        let mut parent_nodes = vec![outer];
                        for &position in address.capture_path() {
                            parent_nodes = parent_nodes
                                .iter()
                                .flat_map(|&node| graph.nodes()[node].sources())
                                .filter(|candidate| candidate.position() == position)
                                .flat_map(|candidate| candidate.captured().iter().copied())
                                .collect();
                            assert!(!parent_nodes.is_empty());
                        }
                        assert!(parent_nodes.iter().any(|&node| {
                            graph.nodes()[node].closure() == closure
                                && graph.nodes()[node].sources().iter().any(|candidate| {
                                    candidate.position() == slot.position()
                                        && candidate.capture().source() == source
                                })
                        }));
                        if closure == graph.nodes()[base].closure() {
                            assert_eq!(source, graph.nodes()[base].sources()[0].capture().source());
                            let (
                                CleanupCaptureValue::Owner(drop_owner),
                                CleanupCaptureValue::Owner(input_owner),
                            ) = (value, base_value)
                            else {
                                panic!("base capture must refer to owned phi source values")
                            };
                            assert_eq!(fact.owner(), Some(drop_owner));
                            assert!(matches!(
                                table.owner_value(drop_owner),
                                Some(CleanupOwnerValue::IterationPhiSourceOwner {
                                    environment,
                                    closure: defined_closure,
                                    source: defined_source,
                                    ..
                                }) if *environment == exit
                                    && *defined_closure == closure
                                    && *defined_source == source
                            ));
                            assert!(matches!(
                                table.owner_value(input_owner),
                                Some(CleanupOwnerValue::IterationPhiSourceOwner {
                                    environment,
                                    closure: defined_closure,
                                    source: defined_source,
                                    ..
                                }) if *environment == header
                                    && *defined_closure == closure
                                    && *defined_source == source
                            ));
                            assert_eq!(slot.position(), base_capture_position);
                            assert_eq!(address.capture_path(), formed_path);
                        }
                        (address.capture_path().to_vec(), slot.position())
                    })
                    .collect::<Vec<_>>();
                let expected_captures = if branch == 0 {
                    vec![(vec![], 1), (vec![0, 0], 0), (vec![0], 0), (vec![], 0)]
                } else {
                    vec![(vec![1, 0], 0), (vec![1], 0), (vec![], 1), (vec![], 0)]
                };
                assert_eq!(captured, expected_captures);
            }
        }
    }

    #[test]
    fn optional_nested_capture_requires_instance_presence() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "optional-nested-capture.ko",
                "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
                    var f: move () -> Unit = move {}
                    for (flag in flags) {
                        val ys = listOf(2)
                        var g: () -> Unit = {}
                        if (flag) { g = { read(ys) } }
                        f = move { val used = g() }
                    }
                    val used = f()
                }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let mut checker =
            super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty());
        let liveness = super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::origins::analyze(&checker).unwrap();
        let mut planner = super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.conditional_nested_phi.is_some());
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement()
            .index();
        let incomings = &planner.loop_phi_incomings[&statement];
        for (kind, boundary) in [
            (
                IterationPhiIncomingKind::Entry,
                IterationPhiBoundary::Header,
            ),
            (
                IterationPhiIncomingKind::Fallthrough,
                IterationPhiBoundary::Header,
            ),
            (
                IterationPhiIncomingKind::Exhaustion,
                IterationPhiBoundary::Exit,
            ),
        ] {
            let edge = incomings.iter().find(|edge| edge.kind() == kind).unwrap();
            let phi = planner.loop_phis[&statement]
                .iter()
                .find(|phi| {
                    phi.boundary() == boundary
                        && sources.slice(names.symbols()[phi.symbol().index()].span()) == Ok("f")
                })
                .unwrap();
            let binding = edge
                .bindings()
                .iter()
                .find(|binding| binding.target() == phi.owner())
                .unwrap();
            assert_eq!(
                binding.presence_source(),
                IterationPhiPresenceSource::CapturedInstances,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn finite_capture_graph_checks_long_chains_without_recursive_traversal() {
        let mut edges = (0..8_192)
            .map(|index| {
                (index < 8_191)
                    .then_some(vec![index + 1])
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        assert_eq!(cyclic_node(&edges), None);
        edges.last_mut().unwrap().push(0);
        assert_eq!(cyclic_node(&edges), Some(0));
    }

    #[test]
    fn finite_layout_order_is_bounded_on_diamond_ladders_and_deep_chains() {
        // 菱形阶梯：每层两个节点都指向下一层两个节点，路径数指数增长，
        // 但有限布局每个节点只记录一次。
        let levels = 64;
        let mut edges = vec![Vec::new(); levels * 2];
        for level in 0..levels - 1 {
            let (left, right) = (level * 2, level * 2 + 1);
            let (next_left, next_right) = ((level + 1) * 2, (level + 1) * 2 + 1);
            edges[left] = vec![next_left, next_right];
            edges[right] = vec![next_left, next_right];
        }
        assert_eq!(finite_layout_order(&edges, &[0, 1]).len(), levels * 2);

        // 8192 深链：迭代遍历，不依赖 Rust 调用栈。
        let depth = 8_192;
        let edges = (0..depth)
            .map(|index| {
                (index < depth - 1)
                    .then_some(vec![index + 1])
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        assert_eq!(finite_layout_order(&edges, &[0]).len(), depth);
    }

    #[test]
    fn recursive_capture_layout_visits_each_static_lambda_once() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "capture-cycle.ko",
                "fun run() { val a = move {}\nval b = move {} }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        let closures = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), Expression::Lambda { .. }).then_some((id, node.span()))
            })
            .collect::<Vec<_>>();
        assert_eq!(closures.len(), 2);
        let mut graph = PhiCaptureGraph::default();
        let first = graph.insert(closures[0].0);
        let second = graph.insert(closures[1].0);
        let first_source = ClosureCaptureSource::Symbol(SymbolId(0));
        let second_source = ClosureCaptureSource::Symbol(SymbolId(1));
        for (node, source, next) in [
            (first, first_source, second),
            (second, second_source, first),
        ] {
            let (closure, span) = closures[node];
            graph.nodes[node].sources.push((
                ClosureCaptureDescriptor::new(
                    closure,
                    source,
                    TypeId::new(0),
                    ClosureCaptureMode::Owned,
                    ClosureCaptureEffect::Move,
                    span,
                ),
                0,
                vec![next],
            ));
        }
        let repeated_source = ClosureCaptureSource::Symbol(SymbolId(2));
        graph.nodes[first].sources.push((
            ClosureCaptureDescriptor::new(
                closures[0].0,
                repeated_source,
                TypeId::new(0),
                ClosureCaptureMode::Owned,
                ClosureCaptureEffect::Move,
                closures[0].1,
            ),
            2,
            vec![second],
        ));
        assert_eq!(
            graph.capture_layout(&[closures[0].0]),
            vec![
                (first, first_source, 0),
                (first, repeated_source, 2),
                (second, second_source, 0)
            ]
        );
        assert_eq!(
            graph.capture_layout(&[closures[1].0]),
            graph.capture_layout(&[closures[0].0])
        );
        let published = graph.published();
        assert_eq!(published.nodes()[first].sources()[0].captured(), &[second]);
        assert_eq!(published.nodes()[first].sources()[1].captured(), &[second]);
        assert_eq!(published.nodes()[second].sources()[0].captured(), &[first]);
        let mut conditions = CleanupConditions::default();
        let owner = conditions.create_owner(CleanupOwnerValue::Closure {
            expression: closures[0].0,
            origin: closures[0].1,
            inputs: Vec::new(),
        });
        let slots = graph.register_capture_layout(&[closures[0].0], owner, &mut conditions);
        assert_eq!(
            slots.len(),
            3,
            "cycle edges must not duplicate static slots"
        );
        for mapped in slots {
            let slot = conditions.capture_slot_value(mapped.slot()).unwrap();
            assert_eq!(slot.environment(), owner);
            assert_eq!(slot.closure(), published.nodes()[mapped.node()].closure());
            assert_eq!(slot.position(), mapped.position());
        }
        graph.nodes[first].sources[0].2 = vec![first];
        graph.nodes[first].sources[1].2 = vec![first];
        assert_eq!(
            graph.capture_layout(&[closures[0].0]),
            vec![(first, first_source, 0), (first, repeated_source, 2)]
        );
    }

    #[test]
    fn self_recursive_capture_layout_registers_finite_static_slots() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "capture-static-cycle.ko",
                "fun run() { val a = move {}\nval b = move {}\nval seed = move {} }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        let closures = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), Expression::Lambda { .. }).then_some((id, node.span()))
            })
            .collect::<Vec<_>>();
        assert_eq!(closures.len(), 3);
        let mut graph = PhiCaptureGraph::default();
        let a = graph.insert(closures[0].0);
        let b = graph.insert(closures[1].0);
        let seed = graph.insert(closures[2].0);
        for (node, candidates) in [(a, vec![a, b, seed]), (b, vec![a, seed])] {
            graph.nodes[node].sources.push((
                ClosureCaptureDescriptor::new(
                    closures[node].0,
                    ClosureCaptureSource::Symbol(SymbolId(node)),
                    TypeId::new(0),
                    ClosureCaptureMode::Owned,
                    ClosureCaptureEffect::Move,
                    closures[node].1,
                ),
                0,
                candidates,
            ));
        }
        assert_eq!(graph.recursive_origin(), Some(closures[a].0));
        let mut conditions = CleanupConditions::default();
        let owner = conditions.create_owner(CleanupOwnerValue::Closure {
            expression: closures[a].0,
            origin: closures[a].1,
            inputs: Vec::new(),
        });
        let layout = graph
            .register_capture_layout(&[closures[a].0], owner, &mut conditions)
            .into_iter()
            .map(|entry| (entry.node(), entry.position(), entry.slot()))
            .collect::<Vec<_>>();
        assert_eq!(layout.len(), 2, "the cycle needs only two static slots");
        assert_eq!(
            layout.iter().map(|(node, _, _)| *node).collect::<Vec<_>>(),
            [a, b]
        );
        for (node, position, slot) in layout {
            let registered = conditions.capture_slot_value(slot).unwrap();
            assert_eq!(registered.environment(), owner);
            assert_eq!(registered.closure(), graph.nodes[node].closure);
            assert_eq!(position, 0, "both captures use original position zero");
            assert_eq!(registered.position(), position);
            assert_eq!(registered.source(), graph.nodes[node].sources[0].0.source());
            assert_eq!(graph.nodes[node].sources[0].2.last(), Some(&seed));
        }
    }
}
