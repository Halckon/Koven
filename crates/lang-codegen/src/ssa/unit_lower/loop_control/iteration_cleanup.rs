//! Consume each frontend cleanup sequence once, preserving provider/call/drop ordering.
use super::{iteration::IterationState, *};
use crate::ssa::model::{EntityId, EntityType, LoanId, Operation};
use lang_frontend::{
    ownership_checking::UnitIterationCleanupAction as Action,
    type_checking::{BorrowReturnOrigin, CallableResultSource, UnitCallReceiverOrigin},
};

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn emit_iteration_cleanup(
        &mut self,
        point: UnitDropPoint,
        actions: &[Action],
    ) -> Result<(), LoweringError> {
        self.begin_nested_map_cleanup(point, actions)?;
        self.begin_conditional_map_cleanup(point)?;
        let drops = actions
            .iter()
            .filter_map(|action| match action {
                Action::Drop(fact) => Some(*fact),
                _ => None,
            })
            .collect::<Vec<_>>();
        self.validate_closure_drop_facts(&drops)?;
        for action in actions {
            match action {
                Action::Drop(fact) => {
                    if !self.emit_nested_guard_drop(*fact)? && !self.emit_map_source_drop(*fact)? {
                        self.emit_drop_fact(*fact)?;
                    }
                }
                Action::DropConditionalReceiver(fact) => {
                    if let Some(owner) = self.conditional_receiver_drop_owner(*fact)? {
                        self.consumed_receiver = self.current_receiver.map(Into::into);
                        self.current_receiver = None;
                        self.function
                            .append_instruction(
                                self.block,
                                Operation::Drop { owner },
                                Vec::new(),
                                Origin::Source(fact.value_origin()),
                            )
                            .map_err(|_| {
                                lowering_error(LoweringErrorKind::InvalidModel, fact.value_origin())
                            })?;
                    }
                }
                Action::EndBinding { statement, symbol } => {
                    let span = self.statement_span(statement.statement())?;
                    match self.iteration_context(*statement, span)?.state {
                        IterationState::Map(_) => {
                            self.map_end_binding(*statement, *symbol, point, span)?
                        }
                        IterationState::Borrowed { element, .. } => {
                            let element = self.iteration_pending_loan(element, span)?;
                            let loan = self.borrow_bindings.remove(symbol).ok_or_else(|| {
                                lowering_error(LoweringErrorKind::MissingFact, span)
                            })?;
                            if loan != element {
                                self.iteration_end_loan(loan, span)?;
                            }
                        }
                        IterationState::Numeric { endpoint_type, .. } => {
                            let Some(LoweredValue::Value(value)) = self.bindings.remove(symbol)
                            else {
                                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                            };
                            if self
                                .function
                                .entity(EntityId::Value(value))
                                .map(|value| value.ty)
                                != Some(EntityType::Value(endpoint_type))
                            {
                                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                            }
                        }
                    }
                }
                Action::EndElement(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    match self.iteration_context(*statement, span)?.state {
                        IterationState::Map(_) => {
                            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                        }
                        IterationState::Borrowed { element, .. } => {
                            let loan = self.iteration_pending_loan(element, span)?;
                            self.iteration_end_loan(loan, span)?;
                        }
                        IterationState::Numeric {
                            current,
                            last,
                            endpoint_type,
                        } => {
                            self.numeric_pending_value(current, endpoint_type, span)?;
                            self.numeric_pending_value(last, endpoint_type, span)?;
                        }
                    }
                }
                Action::FinishProvider(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    if matches!(
                        self.iteration_context(*statement, span)?.state,
                        IterationState::Map(_)
                    ) {
                        self.map_finish_provider(*statement, point, span)?;
                    }
                }
                Action::EndSource(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    if matches!(
                        self.iteration_context(*statement, span)?.state,
                        IterationState::Map(_)
                    ) {
                        self.map_end_source(*statement, point, span)?;
                        continue;
                    }
                    let IterationState::Borrowed { start, .. } =
                        self.iteration_context(*statement, span)?.state
                    else {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    };
                    let loan = self.iteration_pending_loan(start, span)?;
                    self.iteration_end_loan(loan, span)?;
                    if let Some(source) = self
                        .owned
                        .iteration(*statement)
                        .map(|plan| plan.descriptor().source())
                        && self
                            .owned
                            .borrow_results()
                            .range_uses()
                            .iter()
                            .any(|fact| fact.expression() == source)
                    {
                        self.finish_short_range(source.expression(), span)?;
                    }
                }
                Action::EndCallLoan(fact) => {
                    let slots = self
                        .pending_call_frames
                        .iter()
                        .rev()
                        .filter(|frame| frame.call == fact.call() && !frame.receiver)
                        .find_map(|frame| {
                            frame
                                .loan_arguments
                                .iter()
                                .find(|(argument, _)| *argument == fact.argument())
                                .map(|(_, slots)| slots.clone())
                        })
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, fact.begin_span())
                        })?;
                    for slot in slots.into_iter().rev() {
                        let loan = self.iteration_pending_loan(slot, fact.begin_span())?;
                        self.end_short_call_loan(loan, fact.end_span())?;
                    }
                }
                Action::EndReceiverLoan(fact) => {
                    // Trusted range receivers travel as ABI parameter zero in the
                    // argument frame. Use their exact frontend receiver identity;
                    // nominal receiver frames keep their existing cleanup contract.
                    let range_receiver = self.typed.call(fact.call()).is_some_and(|call| {
                        matches!(call.result_source(),
                            CallableResultSource::Carrier(contract)
                                if contract.origin() == BorrowReturnOrigin::Receiver)
                    });
                    let slots = self
                        .pending_call_frames
                        .iter()
                        .rev()
                        .filter(|frame| frame.call == fact.call())
                        .find_map(
                            |frame| match (frame.receiver, range_receiver, fact.source()) {
                                (true, _, _) => Some(frame.created_loans.clone()),
                                (false, true, UnitCallReceiverOrigin::Expression(receiver)) => {
                                    frame
                                        .loan_arguments
                                        .iter()
                                        .find(|(argument, _)| *argument == receiver)
                                        .map(|(_, slots)| slots.clone())
                                }
                                _ => None,
                            },
                        )
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, fact.begin_span())
                        })?;
                    for slot in slots.into_iter().rev() {
                        let loan = self.iteration_pending_loan(slot, fact.begin_span())?;
                        if range_receiver {
                            self.end_short_call_loan(loan, fact.end_span())?;
                        } else {
                            self.iteration_end_loan(loan, fact.end_span())?;
                        }
                    }
                }
                Action::EndCaptureLoan { closure, .. } => {
                    // Unit captured Borrow closures have no runtime representation yet.
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        self.parsed
                            .ast()
                            .expressions()
                            .get(closure.expression())
                            .map_err(|_| LoweringError {
                                kind: LoweringErrorKind::MissingFact,
                                span: None,
                            })?
                            .span(),
                    ));
                }
            }
        }
        self.map_validate_cleanup(point)?;
        self.finish_nested_map_cleanup(point)
    }
    fn iteration_context(
        &self,
        statement: UnitStatementId,
        span: Span,
    ) -> Result<&super::iteration::IterationContext, LoweringError> {
        self.loops
            .iter()
            .rev()
            .filter_map(|context| context.iteration.as_ref())
            .find(|context| context.statement == statement)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }
    fn iteration_pending_loan(&self, index: usize, span: Span) -> Result<LoanId, LoweringError> {
        match self.pending_operands.get(index) {
            Some(EntityId::Loan(loan)) => Ok(*loan),
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }
    pub(super) fn iteration_end_loan(
        &mut self,
        loan: LoanId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.function
            .append_instruction(
                self.block,
                Operation::BorrowEnd { loan },
                Vec::new(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(())
    }
}
