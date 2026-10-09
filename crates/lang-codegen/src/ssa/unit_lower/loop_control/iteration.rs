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
    pub(super) start: usize,
    pub(super) retained_start: usize,
    pub(super) outer_temporaries: BTreeSet<UnitExpressionId>,
    pub(super) element: usize,
    pub(super) step: ValueId,
    pub(super) int_type: SsaTypeId,
}

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn lower_for(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
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
        let container = self.iteration_type(descriptor.source_type(), span)?;
        let element_type = self.iteration_type(descriptor.element_type(), span)?;
        let int_type = self.ssa_builtin(BuiltinType::Int, span)?;
        let bool_type = self.ssa_builtin(BuiltinType::Boolean, span)?;
        let outer_temporaries = self.temporaries.keys().copied().collect();
        let retained_start = self.pending_operands.len();
        let source_loan = if descriptor.provider()
            == lang_frontend::type_checking::IterationProvider::RangeView
        {
            self.range_iteration_loan(source, container, span)?
        } else {
            match plan.source() {
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
            start,
            retained_start,
            outer_temporaries,
            element: element_slot,
            step: header.step,
            int_type,
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
        self.finish_continues(&context)?;
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
    fn iteration_owner_loan(
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
    fn iteration_loan(
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
        let cursor_index = context.start + 2;
        let Some(EntityId::Value(cursor)) = self.pending_operands.get(cursor_index).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(EntityId::Loan(source)) = self.pending_operands.get(context.start).copied() else {
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
            context.step,
            context.int_type,
            &Origin::Source(span),
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.pending_operands[cursor_index] = EntityId::Value(next.value);
        Ok(())
    }
}
