//! Loop statements (`while`, `loop`, sequential container `for`, `RangeView` `for`, and
//! `Map`/`MutableMap` `for`) and ordered exit cleanup in the refined CFG.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as Action,
        UnitIterationSourceAccess,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, IterationProvider, UnitSequentialIterationBinding, UnitStatementId,
    },
};

use super::{Builder, LoopFrame, Lowered, MapForFrame, SeqForFrame, View};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{BinaryOperator, ComparisonOperator, EntityType, LoanKind, ScalarConstant},
    refine::body::{ROp, RTerm},
};

impl Builder<'_, '_> {
    pub(super) fn end_reassigned_loop_views(
        &mut self,
        loop_span: Span,
    ) -> Result<(), LoweringError> {
        if self.active_views.is_empty() {
            return Ok(());
        }
        let mut reassigned = Vec::new();
        for (_, node) in self.inputs.parsed.ast().expressions().iter() {
            let span = node.span();
            if loop_span.source_id() == span.source_id()
                && loop_span.start() <= span.start()
                && span.end() <= loop_span.end()
                && let Expression::Assignment { target, .. } = node.payload()
                && !self.is_field_target(*target)
                && !self.is_element_target(*target)
                && !self.is_map_put_target(*target)
                && let Ok((Expression::Name, target_span)) = self.node(*target)
                && let Ok(symbol) = self.reference(target_span)
            {
                reassigned.push(symbol);
            }
        }
        for symbol in reassigned {
            self.end_views_of(symbol, loop_span)?;
        }
        Ok(())
    }

    pub(super) fn while_loop(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        loop_body: StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let statement = UnitStatementId::new(self.inputs.source_unit, statement);
        self.end_reassigned_loop_views(span)?;
        let header = self.body.block();
        let body_block = self.body.block();
        let finished = self.body.block();
        let exit = self.body.block();
        self.goto(header, span)?;
        self.block = Some(header);
        self.condition(condition, body_block, finished)?;
        let mark = self.enter(body_block);
        self.loops.push(LoopFrame {
            statement,
            continue_target: header,
            exit,
            views: mark,
            loans: self.pending_call_loans.len(),
            broken: false,
            seq_for: None,
            map_for: None,
        });
        let reached = self.statement(loop_body)? != Lowered::Diverged;
        self.loops.pop();
        self.leave(mark, span)?;
        if reached {
            self.goto(header, span)?;
        }
        // The condition-false edge drops at `LoopExit` on its own path; each `break`
        // dropped on its path before jumping to `exit`.
        let facts = self.collect_drops(UnitDropPoint::LoopExit(statement));
        let mark = self.enter(finished);
        self.leave_unclaimed(mark, span)?;
        self.emit_drops(&facts)?;
        self.goto(exit, span)?;
        self.finish_drops(&facts);
        let mark = self.enter(exit);
        self.leave_unclaimed(mark, span)?;
        Ok(Lowered::Unit)
    }

    /// `loop { body }`: its exit is reached only through `break`.
    pub(super) fn loop_statement(
        &mut self,
        statement: StatementId,
        loop_body: StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let statement = UnitStatementId::new(self.inputs.source_unit, statement);
        self.end_reassigned_loop_views(span)?;
        let header = self.body.block();
        let exit = self.body.block();
        self.goto(header, span)?;
        let mark = self.enter(header);
        self.loops.push(LoopFrame {
            statement,
            continue_target: header,
            exit,
            views: mark,
            loans: self.pending_call_loans.len(),
            broken: false,
            seq_for: None,
            map_for: None,
        });
        let reached = self.statement(loop_body)? != Lowered::Diverged;
        let frame = self.loops.pop().expect("loop frame pushed above");
        self.leave(mark, span)?;
        if reached {
            self.goto(header, span)?;
        }
        // Every `break` already dropped at `LoopExit` on its own path.
        let facts = self.collect_drops(UnitDropPoint::LoopExit(statement));
        self.finish_drops(&facts);
        if !frame.broken {
            self.block = None;
            return Ok(Lowered::Diverged);
        }
        let mark = self.enter(exit);
        self.leave_unclaimed(mark, span)?;
        Ok(Lowered::Unit)
    }

    /// `break` / `continue`: pending call loans and views end before drops and branch.
    pub(super) fn loop_jump(
        &mut self,
        expression: ExpressionId,
        is_break: bool,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let frame = self
            .loops
            .last_mut()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        frame.broken |= is_break;
        let frame = *frame;
        let loans_boundary = frame.loans.min(self.pending_call_loans.len());
        while self.pending_call_loans.len() > loans_boundary {
            let loan = self
                .pending_call_loans
                .pop()
                .expect("loans boundary checked");
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        let views_boundary = frame.views.min(self.active_views.len());
        while self.active_views.len() > views_boundary {
            let view = self.active_views.pop().expect("views boundary checked");
            self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), span)?;
        }
        self.drops(UnitDropPoint::ControlTransfer(self.unit(expression)))?;
        if !is_break && let Some(seq) = frame.seq_for {
            let next = self.value(
                ROp::Binary {
                    operator: BinaryOperator::Add,
                    left: seq.cursor,
                    right: seq.step,
                },
                EntityType::Value(seq.int_ty),
                span,
            )?;
            self.push(ROp::Alias { source: next }, vec![seq.cursor], span)?;
        }
        if !is_break && let Some(map_for) = frame.map_for {
            let next = self.value(
                ROp::MapIterationNext {
                    source: map_for.source_loan,
                    cursor: map_for.cursor,
                },
                EntityType::Value(map_for.cursor_ty),
                span,
            )?;
            self.push(ROp::Alias { source: next }, vec![map_for.cursor], span)?;
        }
        if is_break && frame.seq_for.is_none() && frame.map_for.is_none() {
            // This path leaves the loop: owners dying at `LoopExit` drop here, ending any
            // outer view of theirs; the views stay registered for the other paths.
            for (_, target, fact_span) in
                self.collect_drops(UnitDropPoint::LoopExit(frame.statement))
            {
                if matches!(target, UnitDropTarget::Captured { .. }) {
                    continue;
                }
                let UnitDropTarget::Named(symbol) = target else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, fact_span));
                };
                if !self.dead.insert(symbol) {
                    continue;
                }
                let outer: Vec<View> = self.active_views[..views_boundary]
                    .iter()
                    .rev()
                    .copied()
                    .collect();
                for view in outer {
                    if view.symbol == symbol {
                        self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), fact_span)?;
                    }
                }
                let owner = *self
                    .bindings
                    .get(&symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact_span))?;
                self.push(ROp::Drop { owner }, Vec::new(), fact_span)?;
            }
        }
        let target = if is_break {
            frame.exit
        } else {
            frame.continue_target
        };
        self.terminate(RTerm::Goto(target), span)?;
        Ok(Lowered::Diverged)
    }

    pub(super) fn for_statement(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        loop_body: StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let statement_id = UnitStatementId::new(self.inputs.source_unit, statement);
        if self.inputs.owned.map_iteration(statement_id).is_some() {
            return self.map_for(statement, source, loop_body, span);
        }
        let plan = self
            .inputs
            .owned
            .iteration(statement_id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        match plan.descriptor().provider() {
            IterationProvider::NumericRange(_) => {
                self.numeric_for(statement, source, loop_body, span)
            }
            IterationProvider::Sequential(_) | IterationProvider::RangeView => {
                self.sequential_for(statement, source, loop_body, span)
            }
        }
    }

    fn sequential_for(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        loop_body: StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        self.end_reassigned_loop_views(span)?;
        let statement_id = UnitStatementId::new(self.inputs.source_unit, statement);
        let plan = self
            .inputs
            .owned
            .iteration(statement_id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let descriptor = plan.descriptor();
        let is_range_view = descriptor.provider() == IterationProvider::RangeView;
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
        let source_loan = if is_range_view {
            let Some(loan) = self.range_iteration_loan(source, container, span)? else {
                return Ok(Lowered::Diverged);
            };
            loan
        } else {
            match plan.source_access() {
                UnitIterationSourceAccess::Shared => {
                    let symbol = self.named_container_symbol(source, span)?;
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
                    if let Some(view) = self.narrowed_view(source)? {
                        self.value(
                            ROp::SharedReborrow { source: view },
                            EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: container,
                            },
                            span,
                        )?
                    } else {
                        let symbol = self.named_container_symbol(source, span)?;
                        let owner = *self
                            .bindings
                            .get(&symbol)
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        let place = self.value(
                            ROp::RootPlace { owner },
                            EntityType::Place(container),
                            span,
                        )?;
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
                }
                UnitIterationSourceAccess::Temporary => {
                    let owner = match self.expression_inner(source)? {
                        Lowered::Value(slot) => slot,
                        Lowered::Diverged => return Ok(Lowered::Diverged),
                        Lowered::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
                    };
                    self.register_temporary(source, owner)?;
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
            }
        };
        let length_op = if is_range_view {
            ROp::RangeLength { view: source_loan }
        } else {
            ROp::ContainerLength { owner: source_loan }
        };
        let length = self.value(length_op, EntityType::Value(int_ty), span)?;
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
        let elem_place_op = if is_range_view {
            ROp::RangeElementPlace {
                view: source_loan,
                index: cursor,
            }
        } else {
            ROp::ContainerElementPlace {
                owner: source_loan,
                index: cursor,
            }
        };
        let elem_place = self.value(elem_place_op, EntityType::Place(elem_ssa_ty), span)?;
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
        self.loops.push(LoopFrame {
            statement: statement_id,
            continue_target: header,
            exit,
            views: mark,
            loans: self.pending_call_loans.len(),
            broken: false,
            seq_for: Some(SeqForFrame {
                source_loan,
                elem_loan,
                cursor,
                step,
                int_ty,
                range_source: is_range_view.then_some(source),
            }),
            map_for: None,
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

    fn map_for(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        loop_body: StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        self.end_reassigned_loop_views(span)?;
        let statement_id = UnitStatementId::new(self.inputs.source_unit, statement);
        let plan = self
            .inputs
            .owned
            .map_iteration(statement_id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let descriptor = plan.descriptor();
        let container_ty = self.c(descriptor.source_type());
        let key_ty = self.c(descriptor.key_type());
        let value_ty = self.c(descriptor.value_type());
        let container = self.ssa_type(container_ty, span)?;
        let key_ssa = self.ssa_type(key_ty, span)?;
        let value_ssa = self.ssa_type(value_ty, span)?;
        let ulong_unit = self
            .inputs
            .typed
            .types()
            .builtin(BuiltinType::ULong)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let bool_unit = self
            .inputs
            .typed
            .types()
            .builtin(BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let cursor_ty = self.ssa_type(ulong_unit, span)?;
        let bool_ty = self.ssa_type(bool_unit, span)?;
        let source_loan = match plan.source_access() {
            UnitIterationSourceAccess::Shared => {
                let symbol = self.named_container_symbol(source, span)?;
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
                if let Some(view) = self.narrowed_view(source)? {
                    self.value(
                        ROp::SharedReborrow { source: view },
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: container,
                        },
                        span,
                    )?
                } else {
                    let symbol = self.named_container_symbol(source, span)?;
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
            }
            UnitIterationSourceAccess::Temporary => {
                let owner = match self.expression_inner(source)? {
                    Lowered::Value(slot) => slot,
                    Lowered::Diverged => return Ok(Lowered::Diverged),
                    Lowered::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
                };
                self.register_temporary(source, owner)?;
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
        let cursor = self.value(
            ROp::MapIterationFirst {
                source: source_loan,
            },
            EntityType::Value(cursor_ty),
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
            ROp::MapIterationHasNext {
                source: source_loan,
                cursor,
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
        let key_loan = self.value(
            ROp::MapIterationKeyLoan {
                source: source_loan,
                cursor,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: key_ssa,
            },
            span,
        )?;
        let value_loan = self.value(
            ROp::MapIterationValueLoan {
                source: source_loan,
                cursor,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: value_ssa,
            },
            span,
        )?;
        self.borrowed.insert(descriptor.key_binding(), key_loan);
        self.borrowed.insert(descriptor.value_binding(), value_loan);
        self.loops.push(LoopFrame {
            statement: statement_id,
            continue_target: header,
            exit,
            views: mark,
            loans: self.pending_call_loans.len(),
            broken: false,
            seq_for: None,
            map_for: Some(MapForFrame {
                source_loan,
                key_binding: descriptor.key_binding(),
                value_binding: descriptor.value_binding(),
                key_loan,
                value_loan,
                cursor,
                cursor_ty,
            }),
        });
        let reached = self.statement(loop_body)? != Lowered::Diverged;
        self.leave(mark, span)?;
        if reached {
            let next = self.value(
                ROp::MapIterationNext {
                    source: source_loan,
                    cursor,
                },
                EntityType::Value(cursor_ty),
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
                    } else if let Some(map_for) = self.map_for_frame(statement) {
                        let loan = if symbol == map_for.key_binding {
                            map_for.key_loan
                        } else if symbol == map_for.value_binding {
                            map_for.value_loan
                        } else {
                            return Err(error(LoweringErrorKind::MissingFact, span));
                        };
                        self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
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
                Action::FinishProvider(_) | Action::EndCallLoan(_) | Action::EndReceiverLoan(_) => {
                }
                Action::EndSource(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    if let Some(seq) = self.seq_for_frame(statement) {
                        self.push(
                            ROp::BorrowEnd {
                                loan: seq.source_loan,
                            },
                            Vec::new(),
                            span,
                        )?;
                        if let Some(range_source) = seq.range_source
                            && self.short_ranges.contains_key(&self.unit(range_source))
                        {
                            self.finish_short_range(range_source, span)?;
                        }
                    } else if let Some(map_for) = self.map_for_frame(statement) {
                        self.push(
                            ROp::BorrowEnd {
                                loan: map_for.source_loan,
                            },
                            Vec::new(),
                            span,
                        )?;
                    } else {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, self.inputs.span)),
            }
        }
        Ok(true)
    }

    fn seq_for_frame(&self, statement: UnitStatementId) -> Option<SeqForFrame> {
        self.loops
            .iter()
            .rev()
            .find(|frame| frame.statement == statement)
            .and_then(|frame| frame.seq_for)
    }

    fn map_for_frame(&self, statement: UnitStatementId) -> Option<MapForFrame> {
        self.loops
            .iter()
            .rev()
            .find(|frame| frame.statement == statement)
            .and_then(|frame| frame.map_for)
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
