//! 在遍历 body 前分配 header/exit 的 phi 身份，并从动态槽重建状态。
use std::collections::BTreeMap;

use super::{
    super::{
        ClosureOrigin, DropPlanner, OwnedValue, OwnerVersion, OwnershipCheckingError,
        RetainedSource, ValueState,
    },
    capture_graph::{PhiCaptureGraph, finite_layout_order},
};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    ownership_checking::{
        CleanupCaptureInput, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
        CleanupConditions, CleanupOwnerValue, CleanupOwnerValueId, ClosureCaptureMode,
        ClosureCaptureSource, IterationClosurePhiBinding, IterationClosurePhiOrigin,
        IterationClosurePhiSource, IterationPhiBoundary,
    },
    source::Span,
};

#[derive(Clone, Copy)]
struct PhiAllocation {
    statement: StatementId,
    boundary: IterationPhiBoundary,
    origin: Span,
}

struct PhiSeedContext<'a, 'b, 'checker> {
    checker: &'a super::Checker<'checker>,
    current_environment: Option<(CleanupOwnerValueId, ExpressionId)>,
    selected: &'b [&'b IterationClosurePhiBinding],
    statement: StatementId,
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
    /// header/exit 必须在 body 的任何选择、snapshot 或跳转之前取得独立身份。
    pub(super) fn preallocate_closure_phis(
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
    pub(super) fn seed_phi_state(
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
    pub(super) fn exit_binding_required(
        &self,
        statement: StatementId,
        symbol: SymbolId,
        state: &ValueState,
    ) -> bool {
        self.liveness.statement_after[statement.index()].contains(&symbol)
            || (state.position(symbol).is_some()
                && (!self.is_asap_owner(symbol, state)
                    || self.owner_protected_by_context(symbol, state)
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
}
