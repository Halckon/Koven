//! 显式provider frame保持source；退出按词法层次和实际控制点发布顺序。
use super::{
    DropExpressionUse, DropPlanner, PlannerDropFact, PlannerDropPoint, PlannerDropTarget,
    ValueState,
};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::{
        OwnershipCheckingError, UnitIterationCleanupAction as Action,
        UnitIterationExitKind as Exit, UnitIterationExitPlan, UnitIterationOwnershipPlan,
        UnitLoanTarget,
    },
    parser::Expression,
    source::Span,
    type_checking::UnitStatementId,
};

#[derive(Clone, Debug)]
pub(super) struct IterationFrame {
    pub(super) statement: UnitStatementId,
    pub(super) source_root: Option<UnitSymbolId>,
    temporary: Option<(ExpressionId, Span)>,
    element_active: bool,
    scope_depth: usize,
    loop_depth: usize,
}

impl DropPlanner<'_, '_> {
    pub(super) fn iteration(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        // A return/abort in source happens before AcquireProvider.
        if !self.expression(source, DropExpressionUse::Place, state)? {
            return Ok(false);
        }
        let statement_id = UnitStatementId::new(self.checker.source_unit, statement);
        let plan = self.checker.iterations.get(&statement_id).ok_or(
            OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.checker.source_unit.index(),
                expression: source.index(),
            },
        )?;
        let source_root = match plan.source() {
            UnitLoanTarget::Place(place) => Some(place.root()),
            _ => None,
        };
        let temporary = match plan.source() {
            UnitLoanTarget::Temporary(owner) => Some((
                owner.expression(),
                self.checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(owner.expression())?
                    .span(),
            )),
            _ => None,
        };
        self.planned_iterations.insert(statement_id);
        self.iteration_scope_depths
            .insert(statement_id, self.scope_depth);
        state.iterations.push(IterationFrame {
            statement: statement_id,
            source_root,
            temporary,
            element_active: false,
            scope_depth: self.scope_depth,
            loop_depth: self.loop_boundaries.len() + 1,
        });
        let mut body_state = state.clone();
        if let Some(frame) = body_state.iterations.last_mut() {
            frame.element_active = true;
        }
        self.loop_boundaries.push(self.scope_depth);
        if self.statement(body, &mut body_state)? {
            self.end_iteration_element(
                PlannerDropPoint::AfterStatement(body),
                Exit::Fallthrough,
                &mut body_state,
            );
        }
        self.loop_boundaries.pop();
        // Exhaustion is a separate path, not a cleanup continuation of break.
        self.finish_iteration(
            PlannerDropPoint::LoopExit(statement),
            Exit::Exhaustion,
            state,
        );
        self.drop_loop_exit(statement, state);
        Ok(true)
    }

    fn end_iteration_element(
        &mut self,
        point: PlannerDropPoint,
        kind: Exit,
        state: &mut ValueState,
    ) {
        let Some(frame) = state.iterations.last_mut() else {
            return;
        };
        let statement = frame.statement;
        let record = (statement, kind, point);
        if !self.iteration_exits.contains(&record) {
            self.iteration_exits.push(record);
        }
        if !frame.element_active {
            return;
        }
        if let Some(plan) = self.checker.iterations.get(&statement) {
            for binding in plan.bindings().iter().rev() {
                self.iteration_actions.push((
                    point,
                    Action::EndBinding {
                        statement,
                        symbol: binding.symbol(),
                    },
                ));
            }
        }
        self.iteration_actions
            .push((point, Action::EndElement(statement)));
        frame.element_active = false;
    }

    fn finish_iteration(&mut self, point: PlannerDropPoint, kind: Exit, state: &mut ValueState) {
        self.end_iteration_element(point, kind, state);
        let Some(frame) = state.iterations.pop() else {
            return;
        };
        self.iteration_actions
            .push((point, Action::FinishProvider(frame.statement)));
        self.iteration_actions
            .push((point, Action::EndSource(frame.statement)));
        if let Some((temporary, origin)) = frame.temporary {
            self.push_fact(PlannerDropFact::new(
                point,
                PlannerDropTarget::Temporary(temporary),
                origin,
            ));
        }
    }

    /// 最近loop若为while/loop，只清理body；不能误结束外层for。
    pub(super) fn finish_jump_iteration(
        &mut self,
        expression: ExpressionId,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        if state
            .iterations
            .last()
            .is_none_or(|frame| frame.loop_depth != self.loop_boundaries.len())
        {
            return Ok(());
        }
        let point = PlannerDropPoint::ControlTransfer(expression);
        let expression_id = self.checker.unit_expression(expression);
        match self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(expression)?
            .payload()
        {
            Expression::Continue { .. } => {
                self.end_iteration_element(point, Exit::Continue(expression_id), state)
            }
            Expression::Break { .. } => {
                self.finish_iteration(point, Exit::Break(expression_id), state);
                let candidates = state
                    .values
                    .iter()
                    .filter(|value| !self.live_after(point).contains(&value.symbol))
                    .map(|value| value.symbol)
                    .collect::<Vec<_>>();
                for symbol in candidates.into_iter().rev() {
                    self.drop_named_asap(point, symbol, state);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Operand交付之后，从内到外结束provider；外围owner在此后由drop_all清理。
    pub(super) fn finish_return_iterations(
        &mut self,
        expression: ExpressionId,
        state: &mut ValueState,
    ) {
        let point = PlannerDropPoint::ControlTransfer(expression);
        self.end_iteration_pending_loans(point, state, 0);
        while let Some(frame) = state.iterations.last().cloned() {
            // Each pending owner belongs to the loop depth where evaluation formed it.
            // Unwinding an inner provider must not postpone an outer-body receiver past its provider.
            self.drop_pending_temporaries(point, state, |pending| {
                pending.loop_depth >= frame.loop_depth
            });
            self.drop_deeper_than(frame.scope_depth, point, state);
            self.finish_iteration(
                point,
                Exit::Return(self.checker.unit_expression(expression)),
                state,
            );
        }
    }

    /// 只登记已完成求值的实参，不能从同call的后续facts推测其已经建立。
    pub(super) fn register_iteration_pending_loans(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
        state: &mut ValueState,
    ) {
        for loan in self.checker.loans.iter().filter(|loan| {
            loan.call() == self.checker.unit_expression(call)
                && loan.argument() == self.checker.unit_expression(argument)
        }) {
            state.pending_loans.push((
                Action::EndCallLoan(loan.clone()),
                self.loop_boundaries.len(),
            ));
        }
    }

    /// Receiver 已在实参之前求值；其loan与实参一起逆序结束。
    pub(super) fn register_iteration_receiver_loan(
        &self,
        call: ExpressionId,
        state: &mut ValueState,
    ) {
        for fact in self.checker.receiver_facts.iter().filter(|fact| {
            fact.call() == self.checker.unit_expression(call)
                && matches!(
                    fact.kind(),
                    crate::ownership_checking::UnitReceiverOwnershipKind::SharedLoan
                        | crate::ownership_checking::UnitReceiverOwnershipKind::ExclusiveLoan
                )
        }) {
            state.pending_loans.push((
                Action::EndReceiverLoan(fact.clone()),
                self.loop_boundaries.len(),
            ));
        }
    }

    /// 先结束被控制转移放弃的call loan，再允许drop其temporary或结束element来源。
    pub(super) fn end_iteration_pending_loans(
        &mut self,
        point: PlannerDropPoint,
        state: &mut ValueState,
        minimum_depth: usize,
    ) {
        for index in (0..state.pending_loans.len()).rev() {
            if state.pending_loans[index].1 >= minimum_depth {
                let (action, _) = state.pending_loans.remove(index);
                self.iteration_actions.push((point, action));
            }
        }
    }

    /// 用同轮checker模板和实际drop traversal记录组合公开计划，不重新解释AST。
    pub(super) fn iteration_plans(
        &self,
    ) -> Result<Vec<UnitIterationOwnershipPlan>, OwnershipCheckingError> {
        self.planned_iterations
            .iter()
            .map(|statement| {
                let mut plan = self.checker.iterations.get(statement).cloned().ok_or(
                    OwnershipCheckingError::InvalidUnitArgumentPlace {
                        source_unit: statement.source_unit().index(),
                        expression: 0,
                    },
                )?;
                for &(owner, kind, point) in &self.iteration_exits {
                    if owner == *statement {
                        plan.exits.push(UnitIterationExitPlan {
                            kind,
                            point: point.into_unit(self.checker.source_unit),
                            actions: self
                                .iteration_actions
                                .iter()
                                .filter(|(at, _)| *at == point)
                                .map(|(_, action)| action.clone())
                                .collect(),
                        });
                    }
                }
                Ok(plan)
            })
            .collect()
    }
}
