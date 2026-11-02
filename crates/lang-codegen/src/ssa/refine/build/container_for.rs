//! Sequential container `for` iteration and ordered exit cleanup in the refined CFG.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::{
        UnitDropPoint, UnitIterationCleanupAction as Action, UnitIterationSourceAccess,
    },
    source::Span,
    type_checking::{
        BuiltinType, IterationProvider, UnitSequentialIterationBinding, UnitStatementId,
    },
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{BinaryOperator, ComparisonOperator, EntityType, LoanKind, ScalarConstant},
    refine::body::{ROp, RTerm},
};

impl Builder<'_, '_> {
    pub(super) fn for_statement(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        loop_body: StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let statement_id = UnitStatementId::new(self.inputs.source_unit, statement);
        let plan = self
            .inputs
            .owned
            .iteration(statement_id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        match plan.descriptor().provider() {
            IterationProvider::NumericRange(_) => {
                self.numeric_for(statement, source, loop_body, span)
            }
            IterationProvider::Sequential(_) => {
                self.sequential_for(statement, source, loop_body, span)
            }
            IterationProvider::RangeView => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    fn sequential_for(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        loop_body: StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let statement_id = UnitStatementId::new(self.inputs.source_unit, statement);
        let plan = self
            .inputs
            .owned
            .iteration(statement_id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let descriptor = plan.descriptor();
        let container_ty = self.c(descriptor.source_type());
        let element_ty = self.c(descriptor.element_type());
        let container = self.ssa_type(container_ty, span)?;
        let elem_ssa_ty = self.ssa_type(element_ty, span)?;
        let int_unit = self
            .inputs
            .typed
            .types()
            .builtin(BuiltinType::Int)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let bool_unit = self
            .inputs
            .typed
            .types()
            .builtin(BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let int_ty = self.ssa_type(int_unit, span)?;
        let bool_ty = self.ssa_type(bool_unit, span)?;
        let symbol = self.named_container_symbol(source, span)?;
        let source_loan = match plan.source_access() {
            UnitIterationSourceAccess::Shared => {
                let caller_loan = *self
                    .borrowed
                    .get(&symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.value(
                    ROp::SharedReborrow {
                        source: caller_loan,
                    },
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: container,
                    },
                    span,
                )?
            }
            UnitIterationSourceAccess::Owned => {
                let owner = *self
                    .bindings
                    .get(&symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let place =
                    self.value(ROp::RootPlace { owner }, EntityType::Place(container), span)?;
                self.value(
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: container,
                    },
                    span,
                )?
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let length = self.value(
            ROp::ContainerLength { owner: source_loan },
            EntityType::Value(int_ty),
            span,
        )?;
        let cursor = self.value(
            ROp::Constant(ScalarConstant::Integer(0)),
            EntityType::Value(int_ty),
            span,
        )?;
        let step = self.value(
            ROp::Constant(ScalarConstant::Integer(1)),
            EntityType::Value(int_ty),
            span,
        )?;
        let header = self.body.block();
        let body_block = self.body.block();
        let exhausted = self.body.block();
        let exit = self.body.block();
        self.goto(header, span)?;

        let mark = self.enter(header);
        self.leave_unclaimed(mark, span)?;
        let cond = self.value(
            ROp::Compare {
                operator: ComparisonOperator::LessThan,
                left: cursor,
                right: length,
            },
            EntityType::Value(bool_ty),
            span,
        )?;
        self.terminate(
            RTerm::Branch {
                condition: cond,
                when_true: body_block,
                when_false: exhausted,
            },
            span,
        )?;

        let mark = self.enter(body_block);
        let elem_place = self.value(
            ROp::ContainerElementPlace {
                owner: source_loan,
                index: cursor,
            },
            EntityType::Place(elem_ssa_ty),
            span,
        )?;
        let elem_loan = self.value(
            ROp::BorrowBegin {
                place: elem_place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: elem_ssa_ty,
            },
            span,
        )?;
        match descriptor.binding() {
            UnitSequentialIterationBinding::Discard => {}
            UnitSequentialIterationBinding::Name(binding) => {
                self.borrowed.insert(*binding, elem_loan);
            }
            UnitSequentialIterationBinding::Destructure(parts) => {
                for part in parts {
                    let Some(binding) = part.symbol() else {
                        continue;
                    };
                    let part_ssa = self.ssa_type(self.c(part.ty()), span)?;
                    let field = *self
                        .types
                        .field_indices
                        .get(&(element_ty, part.field()))
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let field_loan = self.value(
                        ROp::SharedFieldLoan {
                            base: elem_loan,
                            field,
                        },
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: part_ssa,
                        },
                        span,
                    )?;
                    self.borrowed.insert(binding, field_loan);
                }
            }
        }
        self.loops.push(super::LoopFrame {
            statement: statement_id,
            continue_target: header,
            exit,
            views: mark,
            loans: self.pending_call_loans.len(),
            broken: false,
            seq_for: Some(super::SeqForFrame {
                source_loan,
                elem_loan,
                cursor,
                step,
                int_ty,
            }),
        });
        let reached = self.statement(loop_body)? != Lowered::Diverged;
        self.leave(mark, span)?;
        if reached {
            let next = self.value(
                ROp::Binary {
                    operator: BinaryOperator::Add,
                    left: cursor,
                    right: step,
                },
                EntityType::Value(int_ty),
                span,
            )?;
            self.push(ROp::Alias { source: next }, vec![cursor], span)?;
            self.goto(header, span)?;
        }

        let mark = self.enter(exhausted);
        self.leave_unclaimed(mark, span)?;
        self.drops(UnitDropPoint::LoopExit(statement_id))?;
        self.goto(exit, span)?;
        self.loops.pop();

        let mark = self.enter(exit);
        self.leave_unclaimed(mark, span)?;
        Ok(Lowered::Unit)
    }

    pub(super) fn iteration_cleanup(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<bool, LoweringError> {
        let Some(actions) = self
            .inputs
            .owned
            .iteration_cleanup_at(point)
            .map(<[_]>::to_vec)
        else {
            return Ok(false);
        };
        let is_control_transfer = matches!(point, UnitDropPoint::ControlTransfer(_));
        for action in actions {
            match action {
                Action::Drop(fact) => {
                    let index = self
                        .inputs
                        .owned
                        .drops()
                        .iter()
                        .position(|candidate| *candidate == fact)
                        .ok_or_else(|| {
                            error(LoweringErrorKind::MissingFact, fact.value_origin())
                        })?;
                    let item = [(index, fact.target(), fact.value_origin())];
                    self.emit_drops(&item)?;
                    if is_control_transfer {
                        self.consumed_drops.insert(index);
                    } else {
                        self.finish_drops(&item);
                    }
                }
                Action::EndBinding { statement, symbol } => {
                    let span = self.statement_span(statement.statement())?;
                    if let Some(seq) = self.seq_for_frame(statement) {
                        let loan = *self
                            .borrowed
                            .get(&symbol)
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        if loan != seq.elem_loan {
                            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
                        }
                    }
                }
                Action::EndElement(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    if let Some(seq) = self.seq_for_frame(statement) {
                        self.push(
                            ROp::BorrowEnd {
                                loan: seq.elem_loan,
                            },
                            Vec::new(),
                            span,
                        )?;
                    }
                }
                Action::FinishProvider(_) => {}
                Action::EndSource(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    let seq = self
                        .seq_for_frame(statement)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    self.push(
                        ROp::BorrowEnd {
                            loan: seq.source_loan,
                        },
                        Vec::new(),
                        span,
                    )?;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, self.inputs.span)),
            }
        }
        Ok(true)
    }

    fn seq_for_frame(&self, statement: UnitStatementId) -> Option<super::SeqForFrame> {
        self.loops
            .iter()
            .rev()
            .find(|frame| frame.statement == statement)
            .and_then(|frame| frame.seq_for)
    }

    fn statement_span(&self, statement: StatementId) -> Result<Span, LoweringError> {
        self.inputs
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map(|node| node.span())
            .map_err(|_| error(LoweringErrorKind::MissingFact, self.inputs.span))
    }
}
