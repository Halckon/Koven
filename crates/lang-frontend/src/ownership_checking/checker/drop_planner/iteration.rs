//! Provider 使用期间保留 source owner；提前退出按内层到外层释放临时容器。
//! 有限捕获图、phi 布局/seed 与入边运输分别由私有子模块负责。
mod capture_graph;
mod phi_incoming;
mod phi_state;

use super::{
    Checker, DropFact, DropPlanner, DropPoint, DropTarget, ExpressionUse,
    IterationCleanupAction as Action, IterationExitKind, IterationExitPlan, IterationOwnershipPlan,
    OwnershipCheckingError, ValueState, origins,
};
use crate::ownership_checking::{
    CleanupConditionId, CleanupOwnerValueId, IterationPhiBoundary, IterationPhiIncomingKind,
};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    source::Span,
};

#[cfg(test)]
use capture_graph::{PhiCaptureGraph, PhiCaptureNode, cyclic_node, finite_layout_order};
#[cfg(test)]
use phi_incoming::{coexisting_capture_node, phi_selector_writes};

/// 退出记录在规划时保存路径，产物发布时不得重读后续状态。
#[derive(Clone, Copy, Debug)]
pub(super) struct IterationExitRecord {
    owner: StatementId,
    kind: IterationExitKind,
    point: DropPoint,
    condition: CleanupConditionId,
}

#[derive(Clone, Debug)]
pub(super) struct IterationFrame {
    statement: StatementId,
    bindings_active: bool,
    bindings: Vec<SymbolId>,
    sequential_element: bool,
    borrowed: bool,
    pub(super) source_root: Option<SymbolId>,
    temporary: Option<(ExpressionId, Span, CleanupOwnerValueId)>,
    pub(super) scope_depth: usize,
    pub(super) loop_depth: usize,
}

impl DropPlanner<'_, '_> {
    pub(super) fn iteration(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let borrowed = self
            .checker
            .iterations
            .get(&statement.index())
            .is_none_or(|plan| plan.source().is_some());
        // Source 求值中的 return/abort 发生在 provider 建立之前。
        let source_continues = if self.checker.typed.map_iteration(statement).is_some() {
            self.map_source_expression(source, state)?
        } else {
            self.expression(
                source,
                if borrowed {
                    ExpressionUse::Place
                } else {
                    ExpressionUse::Read
                },
                state,
            )?
        };
        if !source_continues {
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
            .map_iterations
            .get(&statement.index())
            .and_then(|plan| match plan.source() {
                Some(crate::ownership_checking::LoanTarget::Temporary(owner)) => Some(*owner),
                _ => None,
            })
            .or(self
                .checker
                .range_use(source)
                .and_then(|fact| match fact.origin() {
                    crate::ownership_checking::LoanTarget::Temporary(owner) => Some(*owner),
                    _ => None,
                }))
            .or(self.checker.temporary_element_owner(source)?)
            .unwrap_or(source);
        let temporary = if borrowed && self.is_move_only_temporary(temporary_owner) {
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
        let map = self.checker.map_iterations.get(&statement.index());
        let bindings = if let Some(plan) = map {
            plan.bindings()
                .iter()
                .map(|binding| binding.symbol())
                .collect()
        } else {
            self.checker
                .iterations
                .get(&statement.index())
                .map(|plan| {
                    plan.bindings()
                        .iter()
                        .map(|binding| binding.symbol())
                        .collect()
                })
                .unwrap_or_default()
        };
        let source_root = if let Some(plan) = map {
            match plan.source() {
                Some(crate::ownership_checking::LoanTarget::Place(place)) => Some(place.root()),
                _ => None,
            }
        } else if borrowed {
            self.checker.place(source)?.map(|place| place.root())
        } else {
            None
        };
        state.iterations.push(IterationFrame {
            statement,
            bindings_active: false,
            bindings,
            sequential_element: map.is_none(),
            borrowed,
            source_root,
            temporary,
            scope_depth: self.scope_depth,
            loop_depth: self.loop_boundaries.len() + 1,
        });
        let mut body_state = state.clone();
        self.seed_phi_state(statement, IterationPhiBoundary::Header, &mut body_state);
        if let Some(frame) = body_state.iterations.last_mut() {
            frame.bindings_active = true;
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

    fn map_source_expression(
        &mut self,
        expression: ExpressionId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if let Some(receiver) = self.checker.map_source_projection_receiver(expression)? {
            self.map_source_expression(receiver, state)
        } else {
            self.expression(expression, ExpressionUse::Place, state)
        }
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

    pub(super) fn end_iteration_element(
        &mut self,
        point: DropPoint,
        kind: IterationExitKind,
        state: &mut ValueState,
    ) {
        let Some(frame) = state.iterations.last_mut() else {
            return;
        };
        self.iteration_exits.push(IterationExitRecord {
            owner: frame.statement,
            kind,
            point,
            condition: state.path,
        });
        if !frame.bindings_active {
            return;
        }
        for &symbol in frame.bindings.iter().rev() {
            self.cleanup.push((
                point,
                Action::EndBinding {
                    statement: frame.statement,
                    symbol,
                },
            ));
        }
        if frame.sequential_element {
            self.cleanup
                .push((point, Action::EndElement(frame.statement)));
        }
        frame.bindings_active = false;
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
            if frame.borrowed {
                self.cleanup
                    .push((point, Action::EndSource(frame.statement)));
            }
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
            plan.exits = self.recorded_iteration_exits(statement);
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

    fn recorded_iteration_exits(&self, statement: StatementId) -> Vec<IterationExitPlan> {
        let mut exits = Vec::new();
        for &IterationExitRecord {
            owner,
            kind,
            point,
            condition,
        } in &self.iteration_exits
        {
            if owner != statement {
                continue;
            }
            let exit = IterationExitPlan {
                kind,
                condition,
                point,
                actions: self
                    .cleanup
                    .iter()
                    .filter(|(at, _)| *at == point)
                    .map(|(_, action)| *action)
                    .collect(),
            };
            if !exits.contains(&exit) {
                exits.push(exit);
            }
        }
        exits
    }

    pub(super) fn map_iteration_required_exits(
        &self,
    ) -> Vec<crate::ownership_checking::map_iteration::MapIterationRequiredExit> {
        self.iteration_exits
            .iter()
            .filter(|exit| {
                self.checker
                    .map_iterations
                    .contains_key(&exit.owner.index())
            })
            .map(
                |exit| crate::ownership_checking::map_iteration::MapIterationRequiredExit {
                    statement: exit.owner,
                    kind: exit.kind,
                    point: exit.point,
                    condition: exit.condition,
                },
            )
            .collect()
    }

    pub(super) fn map_iteration_plans(
        &self,
    ) -> Vec<crate::ownership_checking::MapIterationOwnershipPlan> {
        if !self.checker.deferred.is_empty() || !self.checker.map_iteration_inputs_valid() {
            return Vec::new();
        }
        let mut plans = Vec::new();
        for template in self.checker.map_iterations.values() {
            let statement = template.descriptor().statement();
            let mut plan = template.clone();
            let Some(origins) = self.loop_origins.get(&statement.index()) else {
                return Vec::new();
            };
            let Some(graph) = self.loop_capture_graphs.get(&statement.index()) else {
                return Vec::new();
            };
            let Some(phis) = self.loop_phis.get(&statement.index()) else {
                return Vec::new();
            };
            plan.closure_flow = origins.clone();
            plan.capture_graph = graph.clone();
            plan.closure_phis = phis.clone();
            plan.closure_phi_incomings = self
                .loop_phi_incomings
                .get(&statement.index())
                .cloned()
                .unwrap_or_default();
            plan.exits = self.recorded_iteration_exits(statement);
            if !plan
                .exits
                .iter()
                .any(|exit| exit.kind() == IterationExitKind::Exhaustion)
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
