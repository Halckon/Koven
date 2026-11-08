//! Provider state occupies stable pending slots so every existing unit CFG rebinds it.
use super::*;
use crate::ssa::{
    model::{EntityId, EntityType, LoanId, LoanKind, Operation, SsaTypeId, ValueId},
    provider,
};
use lang_frontend::{
    ownership_checking::{UnitIterationSourceAccess, UnitLoanTarget},
    type_checking::{BuiltinType, UnitSequentialIterationBinding},
};

pub(super) struct IterationContext {
    pub(super) statement: UnitStatementId,
    pub(super) retained_start: usize,
    pub(super) outer_temporaries: BTreeSet<UnitExpressionId>,
    pub(super) state: IterationState,
}

pub(super) enum IterationState {
    Map(Box<super::map_iteration::MapRuntime>),
    Borrowed {
        start: usize,
        element: usize,
        step: ValueId,
        int_type: SsaTypeId,
    },
    Numeric {
        current: usize,
        last: usize,
        endpoint_type: SsaTypeId,
    },
}

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn lower_for(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_live_if_proof(span)?;
        let statement_id = UnitStatementId::new(self.source_unit, statement);
        let Some(plan) = self.owned.iteration(statement_id).cloned() else {
            // Source control transfer happens before AcquireProvider; no exit plan exists yet.
            return if self.lower(source)? == LoweredValue::Diverged {
                Ok(LoweredValue::Diverged)
            } else {
                Err(lowering_error(LoweringErrorKind::MissingFact, span))
            };
        };
        let descriptor = plan.descriptor();
        if descriptor.source() != UnitExpressionId::new(self.source_unit, source)
            || self.typed.sequential_iteration(statement_id) != Some(descriptor)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if plan.source_access() == UnitIterationSourceAccess::Exclusive {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if matches!(
            descriptor.provider(),
            lang_frontend::type_checking::IterationProvider::NumericRange(_)
        ) {
            if self.typed.map_iterations().is_empty() {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            return self.lower_map_unit_numeric_for(&plan, body, span);
        }
        let container = self.iteration_type(descriptor.source_type(), span)?;
        let element_type = self.iteration_type(descriptor.element_type(), span)?;
        let int_type = self.ssa_builtin(BuiltinType::Int, span)?;
        let bool_type = self.ssa_builtin(BuiltinType::Boolean, span)?;
        let outer_temporaries = self.temporaries.keys().copied().collect();
        let retained_start = self.pending_operands.len();
        if descriptor.provider() == lang_frontend::type_checking::IterationProvider::RangeView {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let source_loan = {
            match plan
                .source()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            {
                UnitLoanTarget::Place(place) => {
                    let symbol = place.root();
                    let symbol_data = self.names.names().source_units()
                        [symbol.source_unit().index()]
                    .resolution()
                    .symbols()
                    .get(symbol.symbol().index())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    if !place.is_root()
                        || symbol_data.kind() == lang_frontend::name_resolution::SymbolKind::Field
                    {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                    match plan.source_access() {
                        UnitIterationSourceAccess::Shared => {
                            let loan = *self.borrow_bindings.get(&symbol).ok_or_else(|| {
                                lowering_error(LoweringErrorKind::MissingFact, span)
                            })?;
                            self.iteration_loan(
                                Operation::SharedReborrow { source: loan },
                                container,
                                span,
                            )?
                        }
                        UnitIterationSourceAccess::Owned => {
                            let Some(LoweredValue::Value(owner)) =
                                self.bindings.get(&symbol).copied()
                            else {
                                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                            };
                            self.iteration_owner_loan(owner, container, span)?
                        }
                        _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
                    }
                }
                UnitLoanTarget::Temporary(_) => {
                    let owner = match self.lower(source)? {
                        LoweredValue::Value(owner) => owner,
                        LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                        LoweredValue::Unit => {
                            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                        }
                    };
                    self.iteration_owner_loan(owner, container, span)?
                }
                UnitLoanTarget::This(_) => {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
        };
        let preheader = self.block;
        let origin = Origin::Source(span);
        let snapshot = provider::snapshot(
            self.function,
            preheader,
            source_loan,
            int_type,
            &origin,
            descriptor.provider(),
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let start = self.pending_operands.len();
        self.pending_operands.extend([
            EntityId::Loan(source_loan),
            EntityId::Value(snapshot.length()),
            EntityId::Value(snapshot.cursor()),
        ]);
        let baseline = self.bindings.clone();
        let mut context = self.create_loop_context(&baseline, span)?;
        let entry = carried_control_edge(context.header, &context.carried, &context.loans);
        let slot = |entity| {
            entry
                .arguments
                .iter()
                .position(|candidate| *candidate == entity)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        };
        let source_slot = slot(EntityId::Loan(source_loan))?;
        let length_slot = slot(EntityId::Value(snapshot.length()))?;
        let cursor_slot = slot(EntityId::Value(snapshot.cursor()))?;
        let header = provider::enter_header(
            self.function,
            preheader,
            context.header,
            snapshot,
            entry.arguments,
            source_slot,
            length_slot,
            cursor_slot,
            &origin,
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = context.header;
        self.bindings = self.rebind_loop(&baseline, self.block, &context, span)?;
        let header_state = self.loop_state();
        let body_block = self.add_carried_control_block(&context.carried, &context.loans, span)?;
        let false_block = self.add_carried_control_block(&context.carried, &context.loans, span)?;
        let body_edge = self.carried_edge_from(
            body_block,
            &context.carried,
            &context.loans,
            &header_state,
            span,
        )?;
        let false_edge = self.carried_edge_from(
            false_block,
            &context.carried,
            &context.loans,
            &header_state,
            span,
        )?;
        let element = provider::guard_and_begin(
            self.function,
            &header,
            bool_type,
            element_type,
            body_edge,
            false_edge,
            &origin,
            descriptor.provider(),
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = body_block;
        self.bindings = self.rebind_loop(&baseline, body_block, &context, span)?;
        let element_slot = self.pending_operands.len();
        self.pending_operands.push(EntityId::Loan(element.loan()));
        context.iteration = Some(IterationContext {
            statement: statement_id,
            retained_start,
            outer_temporaries,
            state: IterationState::Borrowed {
                start,
                element: element_slot,
                step: header.step,
                int_type,
            },
        });
        match descriptor.binding() {
            UnitSequentialIterationBinding::Discard => {}
            UnitSequentialIterationBinding::Name(symbol) => {
                self.borrow_bindings.insert(*symbol, element.loan());
            }
            UnitSequentialIterationBinding::Destructure(parts) => {
                for part in parts {
                    let Some(symbol) = part.symbol() else {
                        continue;
                    };
                    let ty = self.iteration_type(part.ty(), span)?;
                    let concrete = super::super::resolve_concrete_type(
                        self.typed,
                        descriptor.element_type(),
                        self.substitutions,
                        self.static_self,
                        span,
                    )?;
                    let field = *self
                        .field_indices
                        .get(&(concrete, part.field()))
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    let loan = self.iteration_loan(
                        Operation::SharedFieldLoan {
                            base: element.loan(),
                            field,
                        },
                        ty,
                        span,
                    )?;
                    self.borrow_bindings.insert(symbol, loan);
                }
            }
        }
        self.loops.push(context);
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.finish_continues(&context, context.header)?;
        self.block = false_block;
        self.current_receiver = context.entry_receiver;
        self.consumed_receiver = context.entry_consumed_receiver;
        self.bindings = self.rebind_loop(&baseline, false_block, &context, span)?;
        self.closure_bindings = context.entry_closure_bindings.clone();
        self.loops.push(context);
        self.emit_drops(UnitDropPoint::LoopExit(statement_id))?;
        self.pending_operands.truncate(retained_start);
        let false_exit = self.loop_state();
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let mut exits = context
            .breaks
            .into_iter()
            .map(branch_exit)
            .collect::<Vec<_>>();
        exits.push(false_exit);
        self.merge_loop_exits(exits, span)?;
        Ok(LoweredValue::Unit)
    }

    pub(super) fn iteration_type(
        &self,
        ty: lang_frontend::type_checking::UnitTypeId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let concrete = super::super::resolve_concrete_type(
            self.typed,
            ty,
            self.substitutions,
            self.static_self,
            span,
        )?;
        self.type_ids
            .get(&concrete)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }
    pub(super) fn iteration_owner_loan(
        &mut self,
        owner: ValueId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        let (_, result) = self
            .function
            .append_instruction(
                self.block,
                Operation::RootPlace { owner },
                vec![EntityType::Place(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(place) = result[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.iteration_loan(
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            ty,
            span,
        )
    }
    pub(super) fn iteration_loan(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        let (_, result) = self
            .function
            .append_instruction(
                self.block,
                operation,
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        match result[0] {
            EntityId::Loan(loan) => Ok(loan),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }
    pub(super) fn advance_iteration(&mut self, span: Span) -> Result<(), LoweringError> {
        let Some(context) = self
            .loops
            .last()
            .and_then(|context| context.iteration.as_ref())
        else {
            return Ok(());
        };
        if let IterationState::Map(state) = &context.state {
            let state = state.path;
            return self.advance_map_iteration(state, span);
        }
        let IterationState::Borrowed {
            start,
            step,
            int_type,
            ..
        } = context.state
        else {
            // Numeric providers test their terminal value after all continuing edges join.
            return Ok(());
        };
        let cursor_index = start + 2;
        let Some(EntityId::Value(cursor)) = self.pending_operands.get(cursor_index).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(EntityId::Loan(source)) = self.pending_operands.get(start).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let header = self
            .loops
            .last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            .header;
        let next = provider::advance_in_block(
            self.function,
            self.block,
            header,
            source,
            cursor,
            step,
            int_type,
            &Origin::Source(span),
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.pending_operands[cursor_index] = EntityId::Value(next.value);
        Ok(())
    }

    fn lower_map_unit_numeric_for(
        &mut self,
        plan: &lang_frontend::ownership_checking::UnitIterationOwnershipPlan,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        use crate::ssa::model::{BinaryOperator, ComparisonOperator, ScalarConstant};
        use lang_frontend::{
            ownership_checking::IterationSource, type_checking::IterationProvider,
        };
        let descriptor = plan.descriptor();
        let endpoint = match descriptor.provider() {
            IterationProvider::NumericRange(endpoint @ (BuiltinType::Int | BuiltinType::Long)) => {
                endpoint
            }
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let endpoint_type = self.iteration_type(descriptor.element_type(), span)?;
        if plan.source_access() != UnitIterationSourceAccess::Owned
            || plan.source_kind() != &IterationSource::OwnedSnapshot(descriptor.source())
            || endpoint_type != self.ssa_builtin(endpoint, span)?
            || matches!(
                descriptor.binding(),
                UnitSequentialIterationBinding::Destructure(_)
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let range_type = self.iteration_type(descriptor.source_type(), span)?;
        let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
        let source_expr = descriptor.source().expression();
        let source = if let Some(op) = self
            .typed
            .integer_range_operation(descriptor.source())
            .copied()
        {
            let source_span = self
                .parsed
                .ast()
                .expressions()
                .get(source_expr)
                .map_or(span, |n| n.span());
            let LoweredValue::Value(left) = self.lower(op.left().expression())? else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, source_span));
            };
            let LoweredValue::Value(right) = self.lower(op.right().expression())? else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, source_span));
            };
            let (operation, _) = crate::ssa::integer_range::operation(op.kind(), left, right);
            let LoweredValue::Value(value) =
                self.append_scalar(operation, range_type, source_span)?
            else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, source_span));
            };
            self.emit_drops(UnitDropPoint::AfterExpression(descriptor.source()))?;
            value
        } else {
            let LoweredValue::Value(value) = self.lower(source_expr)? else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            value
        };
        let mut proj = |field, ty| -> Result<ValueId, LoweringError> {
            match self.append_scalar(
                Operation::AggregateProject {
                    aggregate: source,
                    field,
                },
                ty,
                span,
            )? {
                LoweredValue::Value(value) => Ok(value),
                _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
            }
        };
        let first = proj(0, endpoint_type)?;
        let last = proj(1, endpoint_type)?;
        let empty = proj(2, boolean)?;
        let retained_start = self.pending_operands.len();
        let outer_temporaries = self.temporaries.keys().copied().collect();
        self.pending_operands
            .extend([EntityId::Value(first), EntityId::Value(last)]);
        let baseline = self.bindings.clone();
        let mut context = self.create_loop_context(&baseline, span)?;
        let exhausted = self.add_carried_control_block(&context.carried, &context.loans, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: empty,
                    when_true: carried_control_edge(exhausted, &context.carried, &context.loans),
                    when_false: carried_control_edge(
                        context.header,
                        &context.carried,
                        &context.loans,
                    ),
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = context.header;
        self.bindings = self.rebind_loop(&baseline, self.block, &context, span)?;
        self.closure_bindings = context.entry_closure_bindings.clone();
        context.iteration = Some(IterationContext {
            statement: descriptor.statement(),
            retained_start,
            outer_temporaries,
            state: IterationState::Numeric {
                current: retained_start,
                last: retained_start + 1,
                endpoint_type,
            },
        });
        if let UnitSequentialIterationBinding::Name(symbol) = descriptor.binding() {
            let Some(EntityId::Value(current)) = self.pending_operands.get(retained_start).copied()
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let LoweredValue::Value(element) =
                self.append_scalar(Operation::Copy { source: current }, endpoint_type, span)?
            else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            self.bindings.insert(*symbol, LoweredValue::Value(element));
        }
        self.loops.push(context);
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        if !context.continues.is_empty() {
            let continuation =
                self.add_carried_control_block(&context.carried, &context.loans, span)?;
            self.finish_continues(&context, continuation)?;
            self.block = continuation;
            self.current_receiver = context.entry_receiver;
            self.consumed_receiver = context.entry_consumed_receiver;
            self.bindings = self.rebind_loop(&baseline, continuation, &context, span)?;
            self.closure_bindings = context.entry_closure_bindings.clone();
            let Some(EntityId::Value(current)) = self.pending_operands.get(retained_start).copied()
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let Some(EntityId::Value(last)) =
                self.pending_operands.get(retained_start + 1).copied()
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let LoweredValue::Value(terminal) = self.append_scalar(
                Operation::Compare {
                    operator: ComparisonOperator::Equal,
                    left: current,
                    right: last,
                },
                boolean,
                span,
            )?
            else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            let increment =
                self.add_carried_control_block(&context.carried, &context.loans, span)?;
            let state = self.loop_state();
            let when_true =
                self.carried_edge_from(exhausted, &context.carried, &context.loans, &state, span)?;
            let when_false =
                self.carried_edge_from(increment, &context.carried, &context.loans, &state, span)?;
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Conditional {
                        condition: terminal,
                        when_true,
                        when_false,
                    },
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            self.block = increment;
            self.bindings = self.rebind_loop(&baseline, increment, &context, span)?;
            let Some(EntityId::Value(current)) = self.pending_operands.get(retained_start).copied()
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let LoweredValue::Value(one) = self.append_scalar(
                Operation::Constant(ScalarConstant::Integer(1)),
                endpoint_type,
                span,
            )?
            else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            let LoweredValue::Value(next) = self.append_scalar(
                Operation::Binary {
                    operator: BinaryOperator::Add,
                    left: current,
                    right: one,
                },
                endpoint_type,
                span,
            )?
            else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            self.pending_operands[retained_start] = EntityId::Value(next);
            let state = self.loop_state();
            let edge = self.carried_edge_from(
                context.header,
                &context.carried,
                &context.loans,
                &state,
                span,
            )?;
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Branch(edge),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        self.block = exhausted;
        self.current_receiver = context.entry_receiver;
        self.consumed_receiver = context.entry_consumed_receiver;
        self.bindings = self.rebind_loop(&baseline, exhausted, &context, span)?;
        self.closure_bindings = context.entry_closure_bindings.clone();
        self.loops.push(context);
        self.emit_drops(UnitDropPoint::LoopExit(descriptor.statement()))?;
        self.pending_operands.truncate(retained_start);
        let exhaustion = self.loop_state();
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let mut exits = context
            .breaks
            .into_iter()
            .map(branch_exit)
            .collect::<Vec<_>>();
        exits.push(exhaustion);
        self.merge_loop_exits(exits, span)?;
        Ok(LoweredValue::Unit)
    }
}
