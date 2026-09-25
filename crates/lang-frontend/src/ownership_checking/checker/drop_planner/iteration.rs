//! Provider 使用期间保留 source owner；提前退出按内层到外层释放临时容器。
use std::collections::BTreeMap;

use super::{
    ClosureOrigin, DropFact, DropPlanner, DropPoint, DropTarget, ExpressionUse,
    IterationCleanupAction as Action, IterationExitKind, IterationExitPlan, IterationOwnershipPlan,
    OwnedValue, OwnerVersion, OwnershipCheckingError, RetainedSource, ValueState,
};
use crate::ownership_checking::{
    CleanupCaptureInput, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
    CleanupConditions, CleanupOwnerValue, CleanupOwnerValueId, ClosureCaptureDescriptor,
    ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource, IterationCaptureGraph,
    IterationCaptureNode, IterationCaptureSource, IterationClosurePhiBinding,
    IterationClosurePhiOrigin, IterationClosurePhiSource, IterationPhiBoundary,
    IterationPhiCaptureSlot, IterationPhiIncoming, IterationPhiIncomingBinding,
    IterationPhiIncomingEnvironment, IterationPhiIncomingKind, IterationPhiIncomingOrigin,
    IterationPhiIncomingSource, IterationPhiIncomingValue,
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
    sources: Vec<(ClosureCaptureDescriptor, usize, Vec<usize>)>,
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
                    sources: node
                        .sources
                        .iter()
                        .map(|(capture, position, captured)| IterationCaptureSource {
                            capture: *capture,
                            position: *position,
                            captured: captured.clone(),
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
            sources: Vec::new(),
        });
        index
    }

    fn expand(&mut self, planner: &DropPlanner<'_, '_>) {
        let mut index = 0;
        while index < self.nodes.len() {
            let closure = self.nodes[index].closure;
            let mut sources = Vec::new();
            for (position, capture) in planner.checker.captures_of(closure).enumerate() {
                if !planner.phi_capture_source_tracked(capture.source()) {
                    continue;
                }
                let captured = planner
                    .phi_capture_origins(closure, capture)
                    .iter()
                    .map(|&next| self.insert(next))
                    .collect();
                sources.push((capture, position, captured));
            }
            self.nodes[index].sources = sources;
            index += 1;
        }
    }

    fn recursive_origin(&self) -> Option<ExpressionId> {
        let edges = self
            .nodes
            .iter()
            .map(|node| {
                node.sources
                    .iter()
                    .flat_map(|(_, _, captured)| captured.iter().copied())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        cyclic_node(&edges).map(|index| self.nodes[index].closure)
    }

    /// 已知子环境与 opaque 值合流，或 tracked 子来源多选，均须读取父实例保存的选择。
    fn conditional_nested_origin(
        &self,
        captured_origins: &super::origins::CapturedOrigins,
    ) -> Option<ExpressionId> {
        self.nodes.iter().find_map(|node| {
            node.sources
                .iter()
                .any(|(capture, _, captured)| {
                    let opaque = captured_origins
                        .get(&(node.closure.index(), capture.source()))
                        .is_some_and(|sources| sources.may_be_opaque);
                    (opaque && !captured.is_empty())
                        || (captured.len() > 1
                            && captured
                                .iter()
                                .any(|&child| !self.nodes[child].sources.is_empty()))
                })
                .then_some(node.closure)
        })
    }
}

fn cyclic_node(edges: &[Vec<usize>]) -> Option<usize> {
    let mut marks = vec![0; edges.len()];
    for root in 0..edges.len() {
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

fn record_phi_origin(
    conditions: &mut CleanupConditions,
    root: CleanupOwnerValueId,
    layout: &IterationClosurePhiOrigin,
    candidates: &[&ClosureOrigin],
    available_when: CleanupConditionId,
    instance_path: Option<(CleanupOwnerValueId, Vec<usize>)>,
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
            if matches!(input.value, CleanupCaptureValue::Environment { .. }) {
                // 基线触发一：父槽自身还有 owned 子环境，无法按实例运输。
                let owned_descendant = slot.is_some_and(|slot| {
                    slot.captured()
                        .iter()
                        .any(|nested| !nested.sources().is_empty())
                });
                // 基线触发二：同一次捕获出现多个候选来源。
                let sibling_alternatives = candidate
                    .captured
                    .iter()
                    .filter(|origin| origin.captured_from == Some(input.source))
                    .take(2)
                    .count()
                    > 1;
                // 放行条件：候选互斥，且父环境已保存形成时的选择（snapshot 或 phi）。
                let readable_from_parent = sibling_alternatives
                    && mutually_exclusive(conditions, &candidate.captured)
                    && candidate.inputs.iter().any(|candidate_input| {
                        matches!(
                            candidate_input.value,
                            CleanupCaptureValue::Environment { .. }
                        ) && (conditions
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
                            ))
                    });
                // 内层形成动作若仍读外层形成前的条件，会在外部选择改变后选错子环境。
                if owned_descendant || (sibling_alternatives && !readable_from_parent) {
                    enclosing_capture_phi.get_or_insert(layout.closure());
                }
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
                .flat_map(|slot| slot.captured())
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

fn seed_phi_origin(
    context: &PhiSeedContext<'_, '_, '_>,
    conditions: &mut CleanupConditions,
    phi_origin: &IterationClosurePhiOrigin,
    owner: CleanupOwnerValueId,
    layout_owner: CleanupOwnerValueId,
    parent_condition: CleanupConditionId,
    retained_sources: &mut Vec<RetainedSource>,
) -> ClosureOrigin {
    let condition = conditions.and(parent_condition, phi_origin.condition());
    let inputs = context
        .checker
        .captures_of(phi_origin.closure())
        .map(|capture| {
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
                        .map(|candidate| candidate.owner())
                }
                _ => None,
            };
            let value = enclosing
                .or_else(|| current.map(CleanupCaptureValue::Owner))
                .or_else(|| slot.map(|slot| CleanupCaptureValue::Owner(slot.owner())))
                .unwrap_or(CleanupCaptureValue::Place(capture.source()));
            if enclosing.is_none()
                && current.is_none()
                && capture.mode() == ClosureCaptureMode::Shared
                && let (ClosureCaptureSource::Symbol(symbol), Some(slot)) = (capture.source(), slot)
            {
                retained_sources.push(RetainedSource {
                    statement: context.statement,
                    owner: slot.owner(),
                    symbol,
                    condition,
                    origin: capture.reference_span(),
                });
            }
            CleanupCaptureInput {
                source: capture.source(),
                value,
                mode: capture.mode(),
                effect: capture.effect(),
                condition,
                origin: capture.reference_span(),
            }
        })
        .collect::<Vec<_>>();
    let captured = phi_origin
        .sources()
        .iter()
        .flat_map(|slot| slot.captured().iter().map(move |origin| (slot, origin)))
        .map(|(slot, origin)| {
            let mut captured = seed_phi_origin(
                context,
                conditions,
                origin,
                slot.owner(),
                layout_owner,
                condition,
                retained_sources,
            );
            captured.captured_from = Some(slot.source());
            captured
        })
        .collect();
    ClosureOrigin {
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
        if let Some(closure) = graph.conditional_nested_origin(&self.captured_origins) {
            self.conditional_nested_phi.get_or_insert(closure);
        }
        self.loop_capture_graphs
            .insert(statement.index(), graph.published());
        let recursive = graph.recursive_origin();
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
                let root_nodes = binding
                    .origins()
                    .iter()
                    .map(|closure| graph.by_closure[&closure.index()])
                    .collect();
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
                let mut origins = Vec::new();
                if recursive.is_none() {
                    for &closure in binding.origins() {
                        origins.push(self.allocate_phi_origin(
                            allocation,
                            &graph,
                            graph.by_closure[&closure.index()],
                            owner,
                            &mut slot,
                        ));
                    }
                }
                phis.push(IterationClosurePhiBinding {
                    boundary,
                    symbol: binding.symbol(),
                    owner,
                    root_nodes,
                    capture_layout,
                    availability_selector,
                    availability_condition,
                    origins,
                });
            }
        }
        if let Some(recursive) = recursive {
            self.recursive_capture_phi.get_or_insert(recursive);
            // 尽管树形来源不可展开，body 仍须从本轮 header owner 读取旧实例。
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

    fn allocate_phi_origin(
        &mut self,
        allocation: PhiAllocation,
        graph: &PhiCaptureGraph,
        node: usize,
        owner: crate::ownership_checking::CleanupOwnerValueId,
        slot: &mut usize,
    ) -> IterationClosurePhiOrigin {
        let closure = graph.nodes[node].closure;
        let condition = self.conditions.iteration_presence(
            allocation.statement,
            allocation.boundary,
            *slot,
            allocation.origin,
        );
        let Some(CleanupCondition::Choice { selector, .. }) = self.conditions.get(condition) else {
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
            let mut captured = Vec::new();
            for &candidate in candidates {
                captured.push(self.allocate_phi_origin(
                    allocation,
                    graph,
                    candidate,
                    source_owner,
                    slot,
                ));
            }
            sources.push(IterationClosurePhiSource {
                source: capture.source(),
                mode: capture.mode(),
                effect: capture.effect(),
                owner: source_owner,
                captured,
            });
        }
        IterationClosurePhiOrigin {
            node,
            closure,
            selector,
            condition,
            sources,
        }
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
                scope_depth,
            });
            let origins = phi
                .origins()
                .iter()
                .map(|phi_origin| {
                    seed_phi_origin(
                        &context,
                        &mut self.conditions,
                        phi_origin,
                        phi.owner(),
                        phi.owner(),
                        state.path,
                        &mut state.retained_sources,
                    )
                })
                .collect::<Vec<_>>();
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
                    || state.replacements.contains(&symbol)))
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
            let values = value
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
            let mut origins = phi
                .origins()
                .iter()
                .map(|origin| {
                    record_phi_origin(
                        &mut self.conditions,
                        phi.owner(),
                        origin,
                        &candidates,
                        available_when,
                        None,
                        &mut self.enclosing_capture_phi,
                    )
                })
                .collect::<Vec<_>>();
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
                capture_slots_to_clear: if phi.origins().is_empty() || coexisting.is_some() {
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
                .map(|header| header.availability_condition())
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
            let mut origins = exit
                .origins()
                .iter()
                .map(|target| {
                    let source = header.and_then(|header| {
                        header
                            .origins()
                            .iter()
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
                    )
                })
                .collect::<Vec<_>>();
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
                capture_slots_to_clear: if exit.origins().is_empty() || coexisting.is_some() {
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

    fn forward_phi_origin(
        context: &mut ForwardPhiContext<'_, '_>,
        root: CleanupOwnerValueId,
        target: &IterationClosurePhiOrigin,
        source: Option<&IterationClosurePhiOrigin>,
        source_owner: Option<crate::ownership_checking::CleanupOwnerValueId>,
        source_layout_owner: Option<CleanupOwnerValueId>,
        instance_path: Option<(CleanupOwnerValueId, Vec<usize>)>,
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
                            .flat_map(|slot| slot.captured())
                            .map(|nested| {
                                let previous = prior.and_then(|prior| {
                                    prior
                                        .captured()
                                        .iter()
                                        .find(|candidate| candidate.closure() == nested.closure())
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
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{DropPoint, DropTarget, PhiCaptureGraph, coexisting_capture_node, cyclic_node};
    use crate::{
        ast::ExpressionId,
        name_resolution::SymbolId,
        ownership_checking::{
            CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
            CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorSource,
            ClosureCaptureDescriptor, ClosureCaptureEffect, ClosureCaptureMode,
            ClosureCaptureSource, IterationCaptureGraph, IterationCaptureNode,
            IterationCaptureSource, IterationCleanupAction, IterationPhiBoundary,
            IterationPhiIncomingBinding, IterationPhiIncomingEnvironment, IterationPhiIncomingKind,
            IterationPhiIncomingOrigin, IterationPhiIncomingSource,
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

    fn replay_edge_presence(
        table: &CleanupConditions,
        edge: &crate::ownership_checking::IterationPhiIncoming,
        choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    ) {
        fn read_origin(
            table: &CleanupConditions,
            origin: &IterationPhiIncomingOrigin,
            before: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
            writes: &mut Vec<(crate::ownership_checking::CleanupSelectorId, usize)>,
        ) {
            writes.push((
                origin.target(),
                usize::from(selected(table, origin.condition(), before)),
            ));
            for child in origin
                .environments()
                .iter()
                .flat_map(|environment| environment.sources())
                .flat_map(|source| source.captured())
            {
                read_origin(table, child, before, writes);
            }
        }

        let before = choices.clone();
        assert!(selected(table, edge.condition(), &before));
        let mut writes = Vec::new();
        for binding in edge.bindings() {
            writes.push((
                binding.availability_selector(),
                usize::from(selected(table, binding.available_when(), &before)),
            ));
            for origin in binding.origins() {
                read_origin(table, origin, &before, &mut writes);
            }
        }
        for (target, value) in writes {
            choices.insert(target, value);
        }
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
    fn conditional_leaf_phi_replays_formed_instance_and_presence() {
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
            fn read_origin(
                table: &CleanupConditions,
                origin: &IterationPhiIncomingOrigin,
                before: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
                writes: &mut Vec<(crate::ownership_checking::CleanupSelectorId, usize)>,
            ) {
                writes.push((
                    origin.target(),
                    usize::from(selected(table, origin.condition(), before)),
                ));
                for child in origin
                    .environments()
                    .iter()
                    .flat_map(|environment| environment.sources())
                    .flat_map(|source| source.captured())
                {
                    read_origin(table, child, before, writes);
                }
            }

            let before = choices.clone();
            let mut writes = vec![(
                binding.availability_selector(),
                usize::from(selected(table, binding.available_when(), &before)),
            )];
            for origin in binding.origins() {
                read_origin(table, origin, &before, &mut writes);
            }
            for (target, value) in writes {
                choices.insert(target, value);
            }
        }

        #[derive(Default)]
        struct EnvironmentInstance {
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
                (closure, self.owners[&callee])
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
                let IterationCleanupAction::CreateClosureOwner { owner, .. } = action else {
                    panic!("formation must start with CreateClosureOwner")
                };
                self.next_instance += 1;
                self.instances
                    .insert(self.next_instance, EnvironmentInstance::default());
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
                let root_write = values
                    .first()
                    .map(|value| (value.source(), old_owners[&value.source()]));
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
                "fun run(flag: Boolean) { val base: move () -> Unit = if (flag) (move {}) else (move {})\nval outer: move () -> Unit = move { var f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
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
        let entry = planner.loop_phi_incomings[&statement.index()]
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
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
        let backedge = planner.loop_phi_incomings[&statement.index()]
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
            .unwrap();
        let carried = backedge
            .bindings()
            .iter()
            .find(|binding| binding.target() == header.owner())
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
            .map(|origin| origin.selector())
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
                        if *callee == outer_snapshot_owner && *closure == outer_closure
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
            replay.copy_phi(&planner.conditions, entry, entry_binding);
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
            let backedge_presence = forwarded_children
                .iter()
                .map(|origin| selected(&planner.conditions, origin.condition(), &replay.choices))
                .collect::<Vec<_>>();
            assert_eq!(backedge_presence, entry_presence);
            assert_eq!(replay.owners[&header.owner()], inner_instance);
            let backedge_source = forwarded
                .sources()
                .iter()
                .find(|source| !source.captured().is_empty())
                .unwrap();
            assert_eq!(
                read_captured(
                    &planner.conditions,
                    backedge_source,
                    &replay.owners,
                    &replay.instances,
                ),
                base_instance
            );
            replay.copy_phi(&planner.conditions, backedge, carried);
            assert_eq!(replay.phi_slots[&header_slot], base_instance);
            for (origin, present) in nested.iter().zip(&entry_presence) {
                assert_eq!(replay.choices[&origin.target()], usize::from(*present));
            }
            let exit_presence = exit_children
                .iter()
                .map(|origin| selected(&planner.conditions, origin.condition(), &replay.choices))
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
            replay.copy_phi(&planner.conditions, exhaustion, exit_binding);
            assert!(!replay.owners.contains_key(&header.owner()));
            assert_eq!(replay.owners[&exit.owner()], inner_instance);
            let exit_capture_slot = exit_source.capture_slot().unwrap();
            assert!(
                exit_binding
                    .capture_slots_to_clear()
                    .contains(&exit_capture_slot)
            );
            assert_eq!(replay.phi_slots[&exit_capture_slot], base_instance);
            assert_eq!(
                replay.instances[&inner_instance].captured[&inner_position],
                base_instance
            );
            for (origin, present) in exit_children.iter().zip(&entry_presence) {
                assert_eq!(replay.choices[&origin.target()], usize::from(*present));
            }
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
        let mut instance_closures = BTreeMap::new();
        let mut values = BTreeMap::new();
        let mut create = |action: IterationCleanupAction, values: &mut BTreeMap<_, _>| {
            let IterationCleanupAction::CreateClosureOwner { owner, closure } = action else {
                panic!("formation must start with CreateClosureOwner")
            };
            next_instance += 1;
            assert!(values.insert(owner, next_instance).is_none());
            assert!(instance_closures.insert(next_instance, closure).is_none());
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
        let seed = create(initial_create, &mut values);
        let entry_source = entry.values()[0].source();
        assert_eq!(values.remove(&entry_source), Some(seed));
        values.insert(entry.target(), seed);
        let zero_round = values.clone();
        let mut captured_slots = BTreeMap::new();
        let mut rounds = Vec::new();
        for _ in 0..2 {
            let new_instance = create(steps[create_at].1, &mut values);
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
            let IterationCleanupAction::CommitOwnerSnapshot { owner, target } = steps[commit_at].1
            else {
                unreachable!()
            };
            assert_eq!((owner, target), (snapshot, committed_symbol));
            let moved = values.remove(&backedge.values()[0].source()).unwrap();
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
        assert_eq!(
            exhausted.values()[0].condition(),
            header.availability_condition()
        );
        let current = values.remove(&exhausted.values()[0].source()).unwrap();
        values.insert(exit.owner(), current);
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
                        statement: release_statement,
                        root,
                    } if *release_statement == statement && *root == root_drop)
            })
            .unwrap()
            .1;
        let IterationCleanupAction::ReleaseClosureInstances {
            statement: release_statement,
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
        let instance_nodes = instance_closures
            .into_iter()
            .map(|(instance, closure)| {
                let node = graph
                    .nodes()
                    .iter()
                    .position(|node| node.closure() == closure)
                    .unwrap();
                (instance, node)
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(instance_nodes[&seed], initial_node);
        assert_eq!(instance_nodes[&rounds[0]], recursive_node);
        assert_eq!(instance_nodes[&rounds[1]], recursive_node);
        // 零轮沿 Exhaustion 根转发，再用已记录的根 drop 释放入口实例。
        let mut zero_round = zero_round;
        let zero_root = zero_round.remove(&exhausted.values()[0].source()).unwrap();
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
        replay_edge_presence(
            &planner.conditions,
            incoming(IterationPhiIncomingKind::Entry),
            &mut choices,
        );
        assert_eq!(choices[&f_header.availability_selector()], 0);
        assert_eq!(choices[&g_header.availability_selector()], 1);
        assert_eq!(owners.remove(&initial_owner), Some(initial_g));
        assert!(owners.insert(g_header.owner(), initial_g).is_none());
        let zero_round = owners.clone();
        let zero_choices = choices.clone();
        let mut captured = BTreeMap::new();
        let mut rounds = Vec::new();
        let mut one_round = None;
        let f_releases = steps
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances { root, .. }
                    if root.owner() == Some(f_header.owner()) =>
                {
                    Some(*root)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(f_releases.len(), 2);
        let f_after_expression = f_releases
            .iter()
            .find(|fact| matches!(fact.point(), DropPoint::AfterExpression(_)))
            .unwrap();
        let f_at_exit = f_releases
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
            replay_edge_presence(
                &planner.conditions,
                incoming(IterationPhiIncomingKind::Fallthrough),
                &mut choices,
            );
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
        let release = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances {
                    statement: loop_id,
                    root,
                } if *loop_id == statement && root.owner() == Some(g_exit.owner()) => Some(*root),
                _ => None,
            })
            .unwrap();
        assert_eq!(release.target(), DropTarget::Named(g_symbol));
        assert!(!steps.iter().any(|(_, action)| {
            matches!(action, IterationCleanupAction::Drop(fact) if *fact == release)
        }));
        let exit_root = |mut owners: BTreeMap<CleanupOwnerValueId, usize>,
                         mut captured: BTreeMap<(usize, usize), usize>,
                         nodes: &BTreeMap<usize, usize>,
                         mut choices: BTreeMap<_, _>| {
            assert!(!selected(
                &planner.conditions,
                f_at_exit.condition().unwrap_or(CleanupConditionId::ALWAYS),
                &choices
            ));
            replay_edge_presence(
                &planner.conditions,
                incoming(IterationPhiIncomingKind::Exhaustion),
                &mut choices,
            );
            let instance = owners.remove(&exhausted_g.values()[0].source()).unwrap();
            assert!(owners.insert(exhausted_g.target(), instance).is_none());
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
            let graph = &planner.loop_capture_graphs[&statement.index()];
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
            let entry = incoming(IterationPhiIncomingKind::Entry);
            let jump_edge = planner.loop_phi_incomings[&statement.index()]
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
                    assert!(binding.origins().is_empty(), "{jump}");
                }
            }
            assert!(binding(entry, f_header.owner()).values().is_empty());
            assert!(
                binding(jump_edge, phi(f_symbol, boundary).owner())
                    .values()
                    .is_empty()
            );
            let steps = &planner.cleanup;
            let formed = |closure| {
                steps
                    .iter()
                    .find_map(|(_, action)| match action {
                        IterationCleanupAction::CreateClosureOwner {
                            owner,
                            closure: formed,
                        } if *formed == closure => Some(*owner),
                        _ => None,
                    })
                    .unwrap()
            };
            let f_owner = formed(graph.nodes()[f_node].closure());
            let g_owner = formed(graph.nodes()[g_node].closure());
            let capture = |owner| {
                steps
                    .iter()
                    .find_map(|(_, action)| match action {
                        IterationCleanupAction::SaveClosureCapture {
                            owner: saved,
                            target,
                            input,
                        } if *saved == owner => Some((*target, *input)),
                        _ => None,
                    })
                    .unwrap()
            };
            let (f_slot, f_input) = capture(f_owner);
            let (g_slot, g_input) = capture(g_owner);
            let snapshot = |closure| {
                steps
                    .iter()
                    .find_map(|(_, action)| match action {
                        IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                            if *value == closure =>
                        {
                            Some(*owner)
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            let f_snapshot = snapshot(graph.nodes()[f_node].closure());
            let g_snapshot = snapshot(graph.nodes()[g_node].closure());
            let action_index = |wanted| {
                steps
                    .iter()
                    .position(|(_, action)| *action == wanted)
                    .unwrap()
            };
            let formation = |closure, owner, slot, input, saved, symbol| {
                let create_at =
                    action_index(IterationCleanupAction::CreateClosureOwner { owner, closure });
                let capture_at = action_index(IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target: slot,
                    input,
                });
                let snapshot_at = steps
                    .iter()
                    .position(|(_, action)| {
                        matches!(action, IterationCleanupAction::SaveOwnerSnapshot {
                            owner: snapshot,
                            value,
                            ..
                        } if *snapshot == saved && *value == closure)
                    })
                    .unwrap();
                let commit_at = action_index(IterationCleanupAction::CommitOwnerSnapshot {
                    owner: saved,
                    target: symbol,
                });
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
                let snapshot = planner.conditions.owner_snapshot(saved).unwrap();
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
            let f_position = planner
                .conditions
                .capture_slot_value(f_slot)
                .unwrap()
                .position();
            let g_position = planner
                .conditions
                .capture_slot_value(g_slot)
                .unwrap()
                .position();
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
            let mut instance_nodes = BTreeMap::from([(1_usize, initial_node)]);
            let mut owners = BTreeMap::from([(initial_owner, 1_usize)]);
            let mut captured = BTreeMap::new();
            let mut choices = BTreeMap::new();
            replay_edge_presence(&planner.conditions, entry, &mut choices);
            assert_eq!(choices[&f_header.availability_selector()], 0, "{jump}");
            assert_eq!(owners.remove(&initial_owner), Some(1));
            assert!(owners.insert(g_header.owner(), 1).is_none());
            let mut next_instance = 1;
            for round in 0..rounds {
                assert!(selected(&planner.conditions, f_input.condition(), &choices));
                next_instance += 1;
                let f_instance = next_instance;
                assert!(instance_nodes.insert(f_instance, f_node).is_none());
                assert!(owners.insert(f_owner, f_instance).is_none());
                let prior_g = owners.remove(&g_header.owner()).unwrap();
                assert!(captured.insert((f_instance, f_position), prior_g).is_none());
                assert_eq!(owners.remove(&f_owner), Some(f_instance));
                assert!(owners.insert(f_snapshot, f_instance).is_none());
                replay_snapshot_choices(&planner.conditions, f_snapshot, &mut choices);
                assert!(selected(&planner.conditions, g_input.condition(), &choices));
                next_instance += 1;
                let g_instance = next_instance;
                assert!(instance_nodes.insert(g_instance, g_node).is_none());
                assert!(owners.insert(g_owner, g_instance).is_none());
                let prior_f = owners.remove(&f_snapshot).unwrap();
                assert!(captured.insert((g_instance, g_position), prior_f).is_none());
                assert_eq!(owners.remove(&g_owner), Some(g_instance));
                assert!(owners.insert(g_snapshot, g_instance).is_none());
                replay_snapshot_choices(&planner.conditions, g_snapshot, &mut choices);
                if jump == "break" {
                    let Some(CleanupCondition::Choice { selector, branches }) =
                        planner.conditions.get(jump_edge.condition())
                    else {
                        panic!("break edge must depend on the current flag")
                    };
                    assert_eq!(
                        branches,
                        &[CleanupConditionId::ALWAYS, CleanupConditionId::NEVER]
                    );
                    let control = planner
                        .conditions
                        .selector(*selector)
                        .unwrap()
                        .control()
                        .unwrap();
                    assert_eq!(
                        sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
                        Ok("if (flag) { break }")
                    );
                    let mut false_choices = choices.clone();
                    false_choices.insert(*selector, 1);
                    assert!(!selected(
                        &planner.conditions,
                        jump_edge.condition(),
                        &false_choices
                    ));
                    choices.insert(*selector, 0);
                } else {
                    assert_eq!(jump_edge.condition(), CleanupConditionId::ALWAYS);
                }
                replay_edge_presence(&planner.conditions, jump_edge, &mut choices);
                assert_eq!(
                    choices[&phi(f_symbol, boundary).availability_selector()],
                    0,
                    "{jump}"
                );
                assert_eq!(owners.remove(&g_snapshot), Some(g_instance));
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
                replay_edge_presence(
                    &planner.conditions,
                    incoming(IterationPhiIncomingKind::Exhaustion),
                    &mut choices,
                );
                let instance = owners.remove(&g_header.owner()).unwrap();
                assert!(owners.insert(g_exit.owner(), instance).is_none());
            }
            let release = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::ReleaseClosureInstances {
                        statement: released_statement,
                        root,
                    } if *released_statement == statement
                        && root.owner() == Some(g_exit.owner()) =>
                    {
                        Some(*root)
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(release.target(), DropTarget::Named(g_symbol));
            assert!(selected(
                &planner.conditions,
                release.condition().unwrap_or(CleanupConditionId::ALWAYS),
                &choices
            ));
            let root = owners.remove(&release.owner().unwrap()).unwrap();
            let released =
                replay_owned_closure_release(graph, &instance_nodes, &mut captured, root);
            assert_eq!(released, (1..=next_instance).collect::<Vec<_>>(), "{jump}");
            assert!(owners.is_empty() && captured.is_empty(), "{jump}");
        }
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
            let header = planner.loop_phis[&statement.index()]
                .iter()
                .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
                .unwrap();
            let target = planner.loop_phis[&statement.index()]
                .iter()
                .find(|phi| phi.boundary() == boundary)
                .unwrap();
            let edge = planner.loop_phi_incomings[&statement.index()]
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
                    planner.conditions.get(edge.condition())
                else {
                    panic!("break must be guarded by the current flag")
                };
                assert_eq!(
                    branches,
                    &[CleanupConditionId::ALWAYS, CleanupConditionId::NEVER]
                );
                let choice = planner.conditions.selector(*selector).unwrap();
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
            let snapshot = planner
                .cleanup
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
            let entry = planner.loop_phi_incomings[&statement.index()]
                .iter()
                .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
                .unwrap();
            let entry = entry
                .bindings()
                .iter()
                .find(|binding| binding.target() == header.owner())
                .unwrap();
            let initial_owner = entry.values()[0].source();
            let formed = planner
                .cleanup
                .iter()
                .filter_map(|(_, action)| match action {
                    IterationCleanupAction::CreateClosureOwner { owner, closure } => {
                        Some((*owner, *closure))
                    }
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>();
            let capture = planner
                .cleanup
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if input.value() == CleanupCaptureValue::Owner(header.owner()) => {
                        Some((*owner, *target, *input))
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(capture.2.condition(), header.availability_condition());
            assert_eq!(capture.2.mode(), ClosureCaptureMode::Owned);
            assert_eq!(capture.2.effect(), ClosureCaptureEffect::Move);
            let slot = planner.conditions.capture_slot_value(capture.1).unwrap();
            assert_eq!(slot.environment(), capture.0);
            assert_eq!(slot.source(), capture.2.source());
            let position = slot.position();
            let snapshot_input = planner.conditions.owner_snapshot(snapshot).unwrap();
            assert_eq!(snapshot_input.capture_inputs().len(), 1);
            assert_eq!(snapshot_input.capture_inputs()[0].owner(), capture.0);
            let create_at = planner
                .cleanup
                .iter()
                .position(|(_, action)| {
                    matches!(action,
                IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == capture.0)
                })
                .unwrap();
            let capture_at = planner
                .cleanup
                .iter()
                .position(|(_, action)| {
                    matches!(action,
                IterationCleanupAction::SaveClosureCapture { owner, .. } if *owner == capture.0)
                })
                .unwrap();
            let snapshot_at = planner
                .cleanup
                .iter()
                .position(|(_, action)| {
                    matches!(action,
                IterationCleanupAction::SaveOwnerSnapshot { owner, .. } if *owner == snapshot)
                })
                .unwrap();
            let commit_at = planner
                .cleanup
                .iter()
                .position(|(_, action)| {
                    matches!(action,
                IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *owner == snapshot && *target == header.symbol())
                })
                .unwrap();
            assert!(create_at < capture_at && capture_at < snapshot_at && snapshot_at < commit_at);
            assert_eq!(planner.cleanup[create_at].0, planner.cleanup[capture_at].0);
            let IterationCleanupAction::SaveOwnerSnapshot {
                condition,
                owner,
                value,
            } = planner.cleanup[snapshot_at].1
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
                assert_eq!(planner.cleanup[index].0, DropPoint::AfterExpression(value));
            }
            let mut owners = BTreeMap::from([(initial_owner, 1_usize)]);
            let mut instance_closures = BTreeMap::from([(1_usize, formed[&initial_owner])]);
            let mut captured = BTreeMap::new();
            assert_ne!(formed[&initial_owner], formed[&capture.0]);
            let seed = owners.remove(&entry.values()[0].source()).unwrap();
            owners.insert(entry.target(), seed);
            let rounds = if jump == "continue" { 2 } else { 1 };
            for instance in 2..=rounds + 1 {
                assert!(
                    instance_closures
                        .insert(instance, formed[&capture.0])
                        .is_none()
                );
                let old = owners.remove(&header.owner()).unwrap();
                assert!(captured.insert((instance, position), old).is_none());
                owners.insert(capture.0, instance);
                let formed_value = owners
                    .remove(&snapshot_input.capture_inputs()[0].owner())
                    .unwrap();
                owners.insert(snapshot, formed_value);
                let incoming = if instance == rounds + 1 {
                    input
                } else {
                    planner.loop_phi_incomings[&statement.index()]
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
                let carried = owners.remove(&incoming.values()[0].source()).unwrap();
                assert!(owners.insert(incoming.target(), carried).is_none());
            }
            let exit = planner.loop_phis[&statement.index()]
                .iter()
                .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
                .unwrap();
            if jump == "continue" {
                let exhaustion = planner.loop_phi_incomings[&statement.index()]
                    .iter()
                    .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
                    .unwrap();
                let forwarded = exhaustion
                    .bindings()
                    .iter()
                    .find(|binding| binding.target() == exit.owner())
                    .unwrap();
                assert_eq!(forwarded.values()[0].source(), header.owner());
                let root = owners.remove(&forwarded.values()[0].source()).unwrap();
                owners.insert(forwarded.target(), root);
            }
            let root_drop = planner
                .facts
                .iter()
                .find(|fact| fact.target() == DropTarget::Named(header.symbol()))
                .unwrap();
            assert_eq!(root_drop.owner(), Some(exit.owner()));
            let release = planner
                .cleanup
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::ReleaseClosureInstances {
                        statement: release_statement,
                        root,
                    } if *release_statement == statement && *root == *root_drop => Some(*action),
                    _ => None,
                })
                .unwrap();
            let IterationCleanupAction::ReleaseClosureInstances {
                statement: release_statement,
                root: release_root,
            } = release
            else {
                unreachable!()
            };
            let graph = &planner.loop_capture_graphs[&release_statement.index()];
            let instance_nodes = instance_closures
                .iter()
                .map(|(&instance, &closure)| {
                    let node = graph
                        .nodes()
                        .iter()
                        .position(|node| node.closure() == closure)
                        .unwrap();
                    (instance, node)
                })
                .collect::<BTreeMap<_, _>>();
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
            assert!(planner.cleanup.iter().any(|(_, action)| matches!(action,
                IterationCleanupAction::ReleaseClosureInstances { statement: release_statement, root: released }
                    if *release_statement == statement && *released == *root)));
            assert!(!planner.cleanup.iter().any(|(_, action)| matches!(action,
                IterationCleanupAction::Drop(fact) if *fact == *root)));
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
        let release = planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances {
                    statement: release_statement,
                    root,
                } if *release_statement == statement
                    && root.owner() == Some(exhausted.target()) =>
                {
                    Some(*action)
                }
                _ => None,
            })
            .unwrap();
        let IterationCleanupAction::ReleaseClosureInstances {
            statement: release_statement,
            root: release_root,
        } = release
        else {
            unreachable!()
        };
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
                &planner.loop_capture_graphs[&release_statement.index()],
                &instance_nodes,
                &mut saved_children,
                root_instance,
            ),
            expected_release
        );
        assert!(saved_children.is_empty() && values.is_empty());
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
        let (statement, graph, parent) = planner
            .loop_capture_graphs
            .iter()
            .find_map(|(statement, graph)| {
                graph.nodes().iter().enumerate().find_map(|(index, node)| {
                    (sources.slice(parsed.ast().expressions().get(node.closure()).ok()?.span())
                        == Ok("move { val a = first()\nval b = second() }"))
                    .then_some((*statement, graph, index))
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
        assert_eq!(
            statement,
            *planner.loop_capture_graphs.keys().max().unwrap()
        );
        let formed = planner
            .cleanup
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
        let saves = planner
            .cleanup
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
            assert_eq!(
                planner
                    .conditions
                    .capture_slot_value(**target)
                    .unwrap()
                    .position(),
                index
            );
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
        let incomings = &planner.loop_phi_incomings[&statement];
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
        let first_loop = *planner.loop_phis.keys().min().unwrap();
        let phis = &planner.loop_phis[&first_loop];
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
        let first_incomings = &planner.loop_phi_incomings[&first_loop];
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
            planner
                .cleanup
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
        for binding in [first_entry, second_entry] {
            let moved = values.remove(&binding.values()[0].source()).unwrap();
            assert!(values.insert(binding.target(), moved).is_none());
        }
        let snapshot_for = |symbol| {
            planner
                .cleanup
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
        let inner_create = planner
            .cleanup
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
        let inner_capture = planner
            .cleanup
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
            planner
                .conditions
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
            planner.conditions.owner_value(xs_owner)
        else {
            panic!("the owned source must come from this round's expression")
        };
        assert_eq!(
            sources.slice(parsed.ast().expressions().get(*xs).unwrap().span()),
            Ok("listOf(1)")
        );
        assert_eq!(
            planner
                .conditions
                .owner_snapshot(second_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            first_header.owner()
        );
        assert_eq!(
            planner
                .conditions
                .owner_snapshot(first_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            inner_owner
        );
        assert_eq!(first_backedge.values()[0].source(), first_snapshot);
        assert_eq!(second_backedge.values()[0].source(), second_snapshot);
        let step = |wanted| {
            planner
                .cleanup
                .iter()
                .position(|(_, action)| *action == wanted)
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
        let second_drop = planner
            .cleanup
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
        let inner_save = planner
            .cleanup
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
            let second_input = planner
                .conditions
                .owner_snapshot(second_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner();
            let moved = values.remove(&second_input).unwrap();
            assert!(values.insert(second_snapshot, moved).is_none());
            let IterationCleanupAction::Drop(old_second) = second_drop else {
                unreachable!()
            };
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
            let CleanupCaptureValue::Owner(source) = input.value() else {
                unreachable!()
            };
            let mut evaluated_source = BTreeMap::from([(xs_owner, 100 + round)]);
            let captured_source = evaluated_source.remove(&source).unwrap();
            assert!(evaluated_source.is_empty());
            let position = planner
                .conditions
                .capture_slot_value(target)
                .unwrap()
                .position();
            assert!(
                leaf_resources
                    .insert((inner, position), captured_source)
                    .is_none()
            );
            let first_input = planner
                .conditions
                .owner_snapshot(first_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner();
            let formed = values.remove(&first_input).unwrap();
            assert_eq!(formed, inner);
            assert!(values.insert(first_snapshot, formed).is_none());
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
            let moved = values.remove(&binding.values()[0].source()).unwrap();
            assert!(values.insert(binding.target(), moved).is_none());
        }
        assert_eq!(values[&first_exit.owner()], first_child);
        assert_eq!(values[&second_exit.owner()], second_child);

        // The second loop must forward the parent handle, never rewrite either saved child edge.
        let parent_instance = create(create_for(formed), &mut values, &mut instance_nodes);
        let mut saved_children = BTreeMap::new();
        for (target, input) in &saves {
            let slot = planner.conditions.capture_slot_value(**target).unwrap();
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
        let parent_snapshot = planner
            .cleanup
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
            planner
                .conditions
                .owner_snapshot(parent_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            formed
        );
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
        assert_eq!(entry.values()[0].source(), parent_snapshot);
        let moved = values.remove(&entry.values()[0].source()).unwrap();
        assert!(values.insert(entry.target(), moved).is_none());
        assert_eq!(backedge.target(), entry.target());
        assert_eq!(backedge.values()[0].source(), entry.target());
        let header = planner.loop_phis[&statement]
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
            .unwrap();
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
            let old = values[&backedge.values()[0].source()];
            assert_eq!(values.insert(backedge.target(), old), Some(old));
            assert_eq!(values[&entry.target()], parent_instance);
            assert_eq!(saved_children, saved_before);
            assert_eq!(saved_children[&(parent_instance, 0)], first_child);
            assert_eq!(saved_children[&(parent_instance, 1)], second_child);
        }
        let moved = values.remove(&exhausted.values()[0].source()).unwrap();
        assert!(values.insert(exhausted.target(), moved).is_none());
        let root_instance = values.remove(&exhausted.target()).unwrap();
        assert_eq!(root_instance, parent_instance);

        // Existing flat DropFacts identify each nested source through the parent instance path.
        let root_drop = planner
            .facts
            .iter()
            .find(|fact| {
                fact.owner() == Some(exhausted.target())
                    && matches!(fact.target(), DropTarget::Named(_))
            })
            .unwrap();
        let release = planner
            .cleanup
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::Drop(fact) if *point == root_drop.point() => Some(*fact),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(release.len(), 5);
        let mut released_resources = Vec::new();
        let mut released_environments = Vec::new();
        for fact in release {
            match fact.target() {
                DropTarget::Captured { .. } => {
                    let address = planner
                        .conditions
                        .instance_address(fact.instance_address().unwrap())
                        .unwrap();
                    assert_eq!(address.root(), exhausted.target());
                    let position = planner
                        .conditions
                        .capture_slot_value(fact.capture_slot().unwrap())
                        .unwrap()
                        .position();
                    match address.capture_path() {
                        [] => {
                            let child = saved_children.remove(&(root_instance, position)).unwrap();
                            assert!(!leaf_resources.contains_key(&(child, 0)));
                            released_environments.push(child);
                        }
                        [child_position] => {
                            let child = saved_children[&(root_instance, *child_position)];
                            assert_eq!(position, 0);
                            released_resources
                                .push(leaf_resources.remove(&(child, position)).unwrap());
                        }
                        path => panic!("unexpected nested capture path: {path:?}"),
                    }
                }
                DropTarget::Named(_) => {
                    assert_eq!(fact, *root_drop);
                    assert!(saved_children.is_empty() && leaf_resources.is_empty());
                    released_environments.push(root_instance);
                }
                target => panic!("unexpected release target: {target:?}"),
            }
        }
        assert_eq!(released_resources, [101, 102]);
        assert_eq!(
            released_environments,
            [second_child, first_child, root_instance]
        );
        assert_eq!(
            dropped_old
                .iter()
                .chain(&released_environments)
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
        let outer_call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "outer()").then_some(id))
            .unwrap();
        let mut addresses = planner
            .cleanup
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::EndCaptureLoan {
                    instance_address, ..
                } if *point == DropPoint::CallReturn(outer_call) => planner
                    .conditions
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
        let mut captured_drops = planner
            .facts
            .iter()
            .filter_map(|fact| match fact.target() {
                DropTarget::Captured { .. }
                    if fact.point() == DropPoint::CallReturn(outer_call) =>
                {
                    let address = planner
                        .conditions
                        .instance_address(fact.instance_address()?)?;
                    let slot = planner
                        .conditions
                        .capture_slot_value(fact.capture_slot()?)?;
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

        let statement = *planner.loop_capture_graphs.keys().max().unwrap();
        let graph = &planner.loop_capture_graphs[&statement];
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
        let first_loop = *planner.loop_phis.keys().min().unwrap();
        let first_phis = &planner.loop_phis[&first_loop];
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
        let first_incomings = &planner.loop_phi_incomings[&first_loop];
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
            planner
                .cleanup
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
        let mut choices = BTreeMap::new();
        replay_edge_presence(
            &planner.conditions,
            first_edge(IterationPhiIncomingKind::Entry),
            &mut choices,
        );
        for incoming in [first_entry, second_entry] {
            let moved = values.remove(&incoming.values()[0].source()).unwrap();
            assert!(values.insert(incoming.target(), moved).is_none());
        }
        let snapshot_for = |target| {
            planner
                .cleanup
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
            planner
                .conditions
                .owner_snapshot(second_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            first_header.owner()
        );
        let inner_create = planner
            .cleanup
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
            planner
                .conditions
                .owner_snapshot(first_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            inner_owner
        );
        let inner_save = planner
            .cleanup
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
        }) = planner.conditions.owner_value(source_owner)
        else {
            panic!("shared source must be the first loop's header phi")
        };
        assert!(
            first_phis
                .iter()
                .any(|phi| phi.owner() == source_owner && phi.symbol() == *source_symbol)
        );
        let source_entry = binding(IterationPhiIncomingKind::Entry, source_owner);
        let [entry_value] = source_entry.values() else {
            panic!("shared source needs one entry value")
        };
        let Some(CleanupOwnerValue::Expression { expression, .. }) =
            planner.conditions.owner_value(entry_value.source())
        else {
            panic!("shared source entry must be the list expression")
        };
        assert_eq!(
            sources.slice(parsed.ast().expressions().get(*expression).unwrap().span()),
            Ok("listOf(1)")
        );
        let source_back = binding(IterationPhiIncomingKind::Fallthrough, source_owner);
        assert_eq!(source_back.values().len(), 1);
        assert_eq!(source_back.values()[0].source(), source_owner);
        assert_eq!(
            planner
                .conditions
                .capture_slot_value(inner_save.0)
                .unwrap()
                .position(),
            0
        );
        let mut source_values = BTreeMap::from([(entry_value.source(), 100_usize)]);
        let source_instance = source_values.remove(&entry_value.source()).unwrap();
        assert!(
            source_values
                .insert(source_owner, source_instance)
                .is_none()
        );
        let mut source_slots = BTreeMap::new();
        let mut live_loans = BTreeMap::from([(source_values[&source_owner], 0_usize)]);
        let mut released_old = Vec::new();
        let old_second_drop = planner
            .cleanup
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
            planner
                .cleanup
                .iter()
                .position(|(_, action)| {
                    matches!(action,
                    IterationCleanupAction::SaveOwnerSnapshot { owner, .. } if *owner == wanted)
                })
                .unwrap()
        };
        let old_drop_index = planner
            .cleanup
            .iter()
            .position(|(_, action)| {
                matches!(action, IterationCleanupAction::Drop(fact) if *fact == old_second_drop)
            })
            .unwrap();
        let early_ends = planner
            .cleanup
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
        let [(early_end_index, Some(early_end_guard))] = early_ends.as_slice() else {
            panic!("replaced second needs one guarded loan end")
        };
        assert!(snapshot_index(second_snapshot) < old_drop_index);
        assert!(old_drop_index < *early_end_index);
        assert!(*early_end_index < snapshot_index(first_snapshot));
        for _ in 0..2 {
            let moved = values.remove(&first_header.owner()).unwrap();
            replay_snapshot_choices(&planner.conditions, second_snapshot, &mut choices);
            assert!(values.insert(second_snapshot, moved).is_none());
            let old = values.remove(&old_second_drop.owner().unwrap()).unwrap();
            released_old.push(old);
            assert!(
                !selected(&planner.conditions, *early_end_guard, &choices),
                "the replaced second has no shared loan on either executed round"
            );
            let formed = create(inner_create, &mut values, &mut instance_closures);
            assert_eq!(instance_closures[&formed], inner.closure());
            assert_eq!(
                inner_save.1.value(),
                CleanupCaptureValue::Owner(source_owner)
            );
            let source = source_values[&source_owner];
            let position = planner
                .conditions
                .capture_slot_value(inner_save.0)
                .unwrap()
                .position();
            assert!(source_slots.insert((formed, position), source).is_none());
            *live_loans.get_mut(&source).unwrap() += 1;
            let formed = values.remove(&inner_owner).unwrap();
            replay_snapshot_choices(&planner.conditions, first_snapshot, &mut choices);
            assert!(values.insert(first_snapshot, formed).is_none());
            replay_edge_presence(
                &planner.conditions,
                first_edge(IterationPhiIncomingKind::Fallthrough),
                &mut choices,
            );
            let writes = [first_back, second_back]
                .into_iter()
                .map(|incoming| {
                    (
                        incoming.target(),
                        values.remove(&incoming.values()[0].source()).unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            for (target, moved) in writes {
                assert!(values.insert(target, moved).is_none());
            }
        }
        assert_eq!(released_old, [initial_second, initial_first]);
        let newer = values[&first_header.owner()];
        let older = values[&second_header.owner()];
        assert_ne!(newer, older);
        replay_edge_presence(
            &planner.conditions,
            first_edge(IterationPhiIncomingKind::Exhaustion),
            &mut choices,
        );
        for incoming in [first_out, second_out] {
            let moved = values.remove(&incoming.values()[0].source()).unwrap();
            assert!(values.insert(incoming.target(), moved).is_none());
        }
        assert_eq!(values[&first_exit.owner()], newer);
        assert_eq!(values[&second_exit.owner()], older);
        let parent_create = planner
            .cleanup
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
        for (_, action) in &planner.cleanup {
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
            let position = planner
                .conditions
                .capture_slot_value(*target)
                .unwrap()
                .position();
            let child = values.remove(&source).unwrap();
            assert!(children.insert(position, child).is_none());
        }
        assert_eq!(children, BTreeMap::from([(0, newer), (1, older)]));
        let parent_snapshot = planner
            .cleanup
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
            planner
                .conditions
                .owner_snapshot(parent_snapshot)
                .unwrap()
                .capture_inputs()[0]
                .owner(),
            parent_owner
        );
        replay_snapshot_choices(&planner.conditions, parent_snapshot, &mut choices);
        let moved = values.remove(&parent_owner).unwrap();
        assert_eq!(moved, parent_instance);
        assert!(values.insert(parent_snapshot, moved).is_none());
        let outer_symbol = planner
            .cleanup
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
        let outer_header = planner.loop_phis[&statement]
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == outer_symbol
            })
            .unwrap();
        let outer_exit = planner.loop_phis[&statement]
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == outer_symbol
            })
            .unwrap();
        let second_incomings = &planner.loop_phi_incomings[&statement];
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
                .filter(|origin| selected(&planner.conditions, origin.condition(), &choices))
            {
                for environment in origin.environments().iter().filter(|environment| {
                    selected(&planner.conditions, environment.condition(), &choices)
                }) {
                    assert_eq!(values[&environment.instance_root()], parent_instance);
                    assert!(environment.capture_path().is_empty());
                    for capture in environment.sources().iter().filter(|capture| {
                        selected(&planner.conditions, capture.input().condition(), &choices)
                    }) {
                        assert!(capture.target().is_none());
                        assert!(capture.capture_slot().is_none());
                        assert!(capture.transport_value().is_none());
                        let (address, slot) = capture.transport_read().unwrap();
                        let address = planner.conditions.instance_address(address).unwrap();
                        assert_eq!(address.root(), environment.instance_root());
                        assert!(address.capture_path().is_empty());
                        let position = planner
                            .conditions
                            .capture_slot_value(slot)
                            .unwrap()
                            .position();
                        let child = children[&position];
                        assert_eq!(instance_closures[&child], inner.closure());
                        assert!(capture.captured().iter().any(|nested| {
                            selected(&planner.conditions, nested.condition(), &choices)
                                && nested.environments().iter().any(|nested_environment| {
                                    selected(
                                        &planner.conditions,
                                        nested_environment.condition(),
                                        &choices,
                                    ) && nested_environment.instance_root()
                                        == environment.instance_root()
                                        && nested_environment.capture_path() == [position]
                                })
                        }));
                        witnessed_children.insert(position);
                    }
                }
            }
            assert_eq!(witnessed_children, BTreeSet::from([0, 1]));
            replay_edge_presence(&planner.conditions, incoming, &mut choices);
            let source = binding.values()[0].source();
            let old = values.remove(&source).unwrap();
            assert_eq!(old, parent_instance);
            assert!(values.insert(binding.target(), old).is_none());
            assert_eq!(children, BTreeMap::from([(0, newer), (1, older)]));
        }
        let root = values.remove(&outer_exit.owner()).unwrap();
        assert_eq!(root, parent_instance);
        assert_eq!(live_loans[&100], 2);

        // Replay only the actions selected by the formed snapshots and both phi loops.
        // The public plan remains deferred; this does not publish executable cleanup.
        let actions = planner
            .cleanup
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
                        .is_some_and(|guard| !selected(&planner.conditions, guard, &choices))
                    {
                        continue;
                    }
                    let address = planner
                        .conditions
                        .instance_address(fact.instance_address().unwrap())
                        .unwrap();
                    assert_eq!(address.root(), outer_exit.owner());
                    assert!(address.capture_path().is_empty());
                    let position = planner
                        .conditions
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
                    if condition
                        .is_some_and(|guard| !selected(&planner.conditions, guard, &choices))
                    {
                        continue;
                    }
                    assert_eq!(captured_source, inner_save.1.source());
                    let address = planner
                        .conditions
                        .instance_address(instance_address)
                        .unwrap();
                    assert_eq!(address.root(), outer_exit.owner());
                    let [position] = address.capture_path() else {
                        panic!("loan must name one child instance")
                    };
                    let child = releasing[position];
                    let capture_position = planner
                        .conditions
                        .capture_slot_value(slot)
                        .unwrap()
                        .position();
                    assert_eq!(capture_position, 0);
                    let layout = planner.conditions.capture_slot_value(slot).unwrap();
                    assert_eq!(layout.closure(), inner.closure());
                    assert_eq!(layout.source(), inner_save.1.source());
                    let CleanupCaptureValue::Owner(static_source) = value else {
                        panic!("tracked source must have a phi owner")
                    };
                    assert!(matches!(
                        planner.conditions.owner_value(static_source),
                        Some(CleanupOwnerValue::IterationPhiSourceOwner { environment, .. })
                            if *environment == owner
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
                    if condition
                        .is_some_and(|guard| !selected(&planner.conditions, guard, &choices))
                    {
                        continue;
                    }
                    let address = planner
                        .conditions
                        .instance_address(instance_address)
                        .unwrap();
                    assert_eq!(address.root(), outer_exit.owner());
                    let [position] = address.capture_path() else {
                        panic!("test must name one child instance")
                    };
                    let child = releasing[position];
                    let layout = planner.conditions.capture_slot_value(capture_slot).unwrap();
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
                    assert!(!possible(&planner.conditions, guard, selector, 0));
                    assert!(possible(&planner.conditions, guard, selector, 1));
                    assert_eq!(selected(&planner.conditions, guard, &choices), last);
                    if selected(&planner.conditions, guard, &choices) {
                        last_loan_guarded_drop_candidates.push(source);
                    }
                }
                IterationCleanupAction::Drop(fact)
                    if matches!(fact.target(), DropTarget::Named(_)) =>
                {
                    if fact
                        .condition()
                        .is_some_and(|guard| !selected(&planner.conditions, guard, &choices))
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
        let graph = &planner.loop_capture_graphs[&statement.index()];
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
        let phis = &planner.loop_phis[&statement.index()];
        let mut path_conditions_by_edge = [Vec::new(), Vec::new()];
        for (index, kind) in [
            IterationPhiIncomingKind::Entry,
            IterationPhiIncomingKind::Exhaustion,
        ]
        .into_iter()
        .enumerate()
        {
            let edge = planner.loop_phi_incomings[&statement.index()]
                .iter()
                .find(|edge| edge.kind() == kind)
                .unwrap();
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
            let mut paths = BTreeSet::new();
            let mut path_conditions = Vec::new();
            let mut reads = 0;
            for origin in binding.origins() {
                paths_to(
                    &planner.conditions,
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
        let (outer_snapshot_index, outer_owner) = planner
            .cleanup
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
        let snapshot = planner.conditions.owner_snapshot(outer_owner).unwrap();
        assert_eq!(snapshot.copies().len(), 3);
        let control = snapshot.copies()[0].source();
        assert!(matches!(
            planner.conditions.selector(control).unwrap().source(),
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
            for (_, action) in planner.cleanup.iter().take(outer_snapshot_index + 1) {
                if let IterationCleanupAction::SaveOwnerSnapshot {
                    condition, owner, ..
                } = action
                {
                    let snapshot = planner.conditions.owner_snapshot(*owner).unwrap();
                    if (*owner == outer_owner
                        || snapshot
                            .copies()
                            .iter()
                            .any(|copy| branch_sources.contains(&copy.target())))
                        && condition
                            .is_none_or(|guard| selected(&planner.conditions, guard, &choices))
                    {
                        replay_snapshot_choices(&planner.conditions, *owner, &mut choices);
                    }
                }
            }
            for rounds in [0, 2] {
                let mut choices = choices.clone();
                let mut values = BTreeMap::new();
                let parent_instance = 100 + branch;
                let entry = planner.loop_phi_incomings[&statement.index()]
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
                        let fallthrough = planner.loop_phi_incomings[&statement.index()]
                            .iter()
                            .find(|edge| edge.kind() == IterationPhiIncomingKind::Fallthrough)
                            .unwrap();
                        for _ in 0..rounds {
                            replay_edge_presence(&planner.conditions, fallthrough, &mut choices);
                            let binding = fallthrough
                                .bindings()
                                .iter()
                                .find(|binding| binding.target() == header)
                                .unwrap();
                            assert_eq!(binding.values().len(), 1);
                            assert_eq!(binding.values()[0].source(), header);
                            assert!(selected(
                                &planner.conditions,
                                binding.values()[0].condition(),
                                &choices
                            ));
                            let current = values.remove(&header).unwrap();
                            assert_eq!(current, parent_instance);
                            assert!(values.insert(header, current).is_none());
                        }
                    }
                    let edge = planner.loop_phi_incomings[&statement.index()]
                        .iter()
                        .find(|edge| edge.kind() == kind)
                        .unwrap();
                    replay_edge_presence(&planner.conditions, edge, &mut choices);
                    let target = if kind == IterationPhiIncomingKind::Entry {
                        header
                    } else {
                        exit
                    };
                    let binding = edge
                        .bindings()
                        .iter()
                        .find(|binding| binding.target() == target)
                        .unwrap();
                    assert_eq!(binding.values().len(), 1);
                    let source = binding.values()[0].source();
                    if kind == IterationPhiIncomingKind::Exhaustion {
                        assert_eq!(source, header);
                    }
                    assert!(selected(
                        &planner.conditions,
                        binding.values()[0].condition(),
                        &choices
                    ));
                    assert_eq!(values.remove(&source), Some(parent_instance));
                    assert!(values.insert(target, parent_instance).is_none());
                    let selected_paths = path_conditions_by_edge[index]
                        .iter()
                        .filter(|(_, condition, _, _, _)| {
                            selected(&planner.conditions, *condition, &choices)
                        })
                        .map(|(path, _, root, _, _)| {
                            assert_eq!(*root, source);
                            assert_eq!(values[&target], parent_instance);
                            path.clone()
                        })
                        .collect::<BTreeSet<_>>();
                    assert_eq!(
                        selected_paths,
                        BTreeSet::from([expected.clone()]),
                        "{kind:?}, branch {branch}, rounds {rounds}"
                    );
                }
                let root_drop = planner
                    .facts
                    .iter()
                    .find(|fact| {
                        fact.owner() == Some(exit) && matches!(fact.target(), DropTarget::Named(_))
                    })
                    .unwrap();
                let (base_value, base_owner) = path_conditions_by_edge[1]
                    .iter()
                    .find(|(_, condition, _, _, _)| {
                        selected(&planner.conditions, *condition, &choices)
                    })
                    .map(|(_, _, _, value, owner)| (*value, *owner))
                    .unwrap();
                let selected_drops = planner
                    .cleanup
                    .iter()
                    .filter(|(point, _)| *point == root_drop.point())
                    .filter_map(|(_, action)| match action {
                        IterationCleanupAction::Drop(fact)
                            if fact.condition().is_none_or(|guard| {
                                selected(&planner.conditions, guard, &choices)
                            }) =>
                        {
                            Some(*fact)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(selected_drops.last(), Some(root_drop));
                let captured = selected_drops[..selected_drops.len() - 1]
                    .iter()
                    .map(|fact| {
                        let DropTarget::Captured {
                            owner,
                            closure,
                            source,
                            value,
                            ..
                        } = fact.target()
                        else {
                            panic!("only the root drop may be named")
                        };
                        let address = planner
                            .conditions
                            .instance_address(fact.instance_address().unwrap())
                            .unwrap();
                        assert_eq!(address.root(), exit);
                        let slot = planner
                            .conditions
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
                                planner.conditions.owner_value(drop_owner),
                                Some(CleanupOwnerValue::IterationPhiSourceOwner {
                                    environment,
                                    closure: defined_closure,
                                    source: defined_source,
                                    ..
                                }) if *environment == owner
                                    && *defined_closure == closure
                                    && *defined_source == source
                            ));
                            assert!(matches!(
                                planner.conditions.owner_value(input_owner),
                                Some(CleanupOwnerValue::IterationPhiSourceOwner {
                                    environment,
                                    closure: defined_closure,
                                    source: defined_source,
                                    ..
                                }) if *environment == base_owner
                                    && *defined_closure == closure
                                    && *defined_source == source
                            ));
                            assert_eq!(slot.position(), base_capture_position);
                            assert_eq!(address.capture_path(), expected);
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
