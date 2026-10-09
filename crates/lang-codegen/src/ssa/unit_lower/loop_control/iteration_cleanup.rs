//! Consume each frontend cleanup sequence once, preserving provider/call/drop ordering.
use super::*;
use crate::ssa::model::{EntityId, LoanId, Operation};
use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn emit_iteration_cleanup(
        &mut self,
        actions: &[Action],
    ) -> Result<(), LoweringError> {
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
                Action::Drop(fact) => self.emit_drop_fact(*fact)?,
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
                    let context = self.iteration_context(*statement, span)?;
                    let element = self.iteration_pending_loan(context.element, span)?;
                    let loan = self
                        .borrow_bindings
                        .remove(symbol)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    if loan != element {
                        self.iteration_end_loan(loan, span)?;
                    }
                }
                Action::EndElement(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    let context = self.iteration_context(*statement, span)?;
                    let loan = self.iteration_pending_loan(context.element, span)?;
                    self.iteration_end_loan(loan, span)?;
                }
                Action::FinishProvider(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    self.iteration_context(*statement, span)?;
                }
                Action::EndSource(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    let context = self.iteration_context(*statement, span)?;
                    let loan = self.iteration_pending_loan(context.start, span)?;
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
                    let frame = self
                        .pending_call_frames
                        .iter()
                        .rev()
                        .find(|frame| frame.call == fact.call() && frame.receiver)
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, fact.begin_span())
                        })?;
                    let slots = frame.created_loans.clone();
                    for slot in slots.into_iter().rev() {
                        let loan = self.iteration_pending_loan(slot, fact.begin_span())?;
                        self.iteration_end_loan(loan, fact.end_span())?;
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
        Ok(())
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
    fn iteration_end_loan(&mut self, loan: LoanId, span: Span) -> Result<(), LoweringError> {
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
