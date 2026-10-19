//! Ordered Map providers carry one source lease and an opaque u64 cursor in pending slots.
mod binding_choice;
mod carrier;
mod checked;
mod cleanup;
mod conditional;
mod local;
mod source;
mod state;
mod transfer;
mod transport;
use super::super::map_iteration;
use super::{
    iteration::{IterationContext, IterationState},
    *,
};
use crate::ssa::model::{
    EntityId, EntityType, LoanId, LoanKind, MapContainerKind, Operation, SsaTypeId, SsaTypeKind,
    ValueId,
};
use carrier::MapCarrier;
use conditional::ConditionalState;
use lang_frontend::{
    ownership_checking::{UnitIterationExitKind, UnitIterationSourceAccess},
    type_checking::{BuiltinType, MapIterationKind},
};
use map_iteration::SourceIdentity;
pub(super) use state::MapRuntime;

#[derive(Clone, Copy)]
pub(super) enum MapPhase {
    Body {
        key_live: bool,
        value_live: bool,
    },
    Terminal {
        point: UnitDropPoint,
        finished: bool,
        source_ended: bool,
        owner_dropped: bool,
    },
}

#[derive(Clone, Copy)]
pub(super) struct MapState {
    source_slot: usize,
    cursor_slot: usize,
    key_slot: usize,
    value_slot: usize,
    key_binding: UnitSymbolId,
    value_binding: UnitSymbolId,
    cursor_type: SsaTypeId,
    carrier: MapCarrier,
    phase: MapPhase,
    conditional: Option<ConditionalState>,
}

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn lower_map_for(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_live_if_proof(span)?;
        self.finish_local_map_continuation(span)?;
        if !self.pending_call_frames.is_empty() || !self.nullable_proofs.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_map_parent_temporaries(span)?;
        let statement_id = UnitStatementId::new(self.source_unit, statement);
        let plan = self
            .owned
            .map_iteration(statement_id)
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let descriptor = plan.descriptor();
        if descriptor.source() != UnitExpressionId::new(self.source_unit, source) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let qualified =
            map_iteration::qualify(self.parsed, self.names, self.typed, self.owned, descriptor)?;
        self.validate_map_nesting(qualified, span)?;
        if qualified.nested.is_some()
            && (!self.capture_loans.is_empty()
                || !self.result_source_loans.is_empty()
                || !self.temporary_range_slots.is_empty()
                || !self.short_range_ends.is_empty())
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let container = self.iteration_type(descriptor.source_type(), span)?;
        let key_type = self.iteration_type(descriptor.key_type(), span)?;
        let value_type = self.iteration_type(descriptor.value_type(), span)?;
        let kind = match descriptor.kind() {
            MapIterationKind::Map => MapContainerKind::Map,
            MapIterationKind::MutableMap => MapContainerKind::MutableMap,
        };
        // This is the exact Module type table frozen by orchestrate, not an inferred layout.
        if self.ssa_types.get(container.index())
            != Some(&SsaTypeKind::MapContainer {
                kind,
                key: key_type,
                value: value_type,
            })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let cursor_type = self.ssa_builtin(BuiltinType::ULong, span)?;
        let bool_type = self.ssa_builtin(BuiltinType::Boolean, span)?;
        // Snapshot the outer frame before evaluating the hidden source owner.
        let outer_temporaries = self.temporaries.keys().copied().collect();
        let (entry, source_loan) =
            self.map_source_seed(qualified, plan.source_access(), container, span)?;
        let parent_checkpoint = self.checkpoint_map_stack(span)?;
        let preheader = self.block;
        let cursor = self.map_iteration_value(
            Operation::MapIterationFirst {
                source: source_loan,
            },
            cursor_type,
            span,
        )?;
        let retained_start = self.pending_operands.len();
        let source_slot = self.pending_operands.len();
        let cursor_slot = source_slot + 1;
        self.pending_operands
            .extend([EntityId::Loan(source_loan), EntityId::Value(cursor)]);
        let baseline = self.bindings.clone();
        let mut context = self.create_loop_context(&baseline, span)?;
        let entry_edge = carried_control_edge(context.header, &context.carried, &context.loans);
        self.function
            .set_terminator(
                preheader,
                TerminatorKind::Branch(entry_edge.clone()),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = context.header;
        self.bindings = self.rebind_loop(&baseline, self.block, &context, span)?;
        let parent_header = self.transport_map_stack(&parent_checkpoint, &entry_edge, span)?;
        let header_state = self.loop_state();
        let body_block = self.add_carried_control_block(&context.carried, &context.loans, span)?;
        let false_block = self.add_carried_control_block(&context.carried, &context.loans, span)?;
        let true_edge = self.carried_edge_from(
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
        let header_source = self.map_pending_source(source_slot, span)?;
        let header_cursor = self.map_pending_cursor(cursor_slot, cursor_type, span)?;
        let has_next = self.map_iteration_value(
            Operation::MapIterationHasNext {
                source: header_source,
                cursor: header_cursor,
            },
            bool_type,
            span,
        )?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: has_next,
                    when_true: true_edge.clone(),
                    when_false: false_edge.clone(),
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let birth = carrier::Birth {
            qualified,
            seed: entry,
            source: source_loan,
            cursor,
            preheader,
            header: context.header,
            body: body_block,
            exhaustion: false_block,
            edge: &entry_edge,
            span,
        };
        let carrier = MapCarrier::new(self, &birth)?;
        // Save the real false-edge owner/source state before body cleanup changes bindings.
        self.restore_loop_state(&header_state);
        self.block = false_block;
        self.bindings = self.rebind_loop(&baseline, false_block, &context, span)?;
        carrier.validate_current(self, source_slot, cursor_slot, false_block, span)?;
        let parent_false = self.transport_map_stack(&parent_header, &false_edge, span)?;
        let false_state = self.loop_state();
        self.restore_loop_state(&header_state);
        self.block = body_block;
        self.bindings = self.rebind_loop(&baseline, body_block, &context, span)?;
        carrier.validate_current(self, source_slot, cursor_slot, body_block, span)?;
        self.transport_map_stack(&parent_header, &true_edge, span)?;
        let source = self.map_pending_source(source_slot, span)?;
        let cursor = self.map_pending_cursor(cursor_slot, cursor_type, span)?;
        let key = self.iteration_loan(
            Operation::MapIterationKeyLoan { source, cursor },
            key_type,
            span,
        )?;
        let value = self.iteration_loan(
            Operation::MapIterationValueLoan { source, cursor },
            value_type,
            span,
        )?;
        let key_slot = self.pending_operands.len();
        let value_slot = key_slot + 1;
        self.pending_operands
            .extend([EntityId::Loan(key), EntityId::Loan(value)]);
        if self
            .borrow_bindings
            .insert(descriptor.key_binding(), key)
            .is_some()
            || self
                .borrow_bindings
                .insert(descriptor.value_binding(), value)
                .is_some()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let conditional = qualified
            .conditional
            .map(|shape| {
                ConditionalState::new(
                    self,
                    shape,
                    source_slot,
                    cursor_slot,
                    cursor_type,
                    key,
                    value,
                    span,
                )
            })
            .transpose()?;
        context.iteration = Some(IterationContext {
            statement: statement_id,
            retained_start,
            outer_temporaries,
            state: IterationState::Map(Box::new(MapRuntime {
                path: MapState {
                    source_slot,
                    cursor_slot,
                    key_slot,
                    value_slot,
                    key_binding: descriptor.key_binding(),
                    value_binding: descriptor.value_binding(),
                    cursor_type,
                    carrier,
                    conditional,
                    phase: MapPhase::Body {
                        key_live: true,
                        value_live: true,
                    },
                },
                evidence: conditional::Evidence::default(),
            })),
            inherited_borrowed: None,
        });
        self.loops.push(context);
        // A direct tail transfer already consumes its own cleanup. Never synthesize
        // Fallthrough/Next after it; the independent false edge still needs lowering.
        let result = self.lower_statement(body)?;
        if result != LoweredValue::Diverged {
            if qualified.body_exit != UnitIterationExitKind::Fallthrough {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.record_natural_continue(span)?;
        } else if qualified.conditional.is_none()
            && qualified.body_exit == UnitIterationExitKind::Fallthrough
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let mut context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (breaks, continues) = if qualified.conditional.is_some() {
            (context.breaks.len(), context.continues.len())
        } else {
            match qualified.body_exit {
                UnitIterationExitKind::Fallthrough | UnitIterationExitKind::Continue(_) => (0, 1),
                UnitIterationExitKind::Break(_) => (1, 0),
                UnitIterationExitKind::Return(_) => (0, 0),
                UnitIterationExitKind::Exhaustion => {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
        };
        let current_carrier = match context.iteration.as_ref().map(|iteration| &iteration.state) {
            Some(IterationState::Map(state)) => state.carrier,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if context.breaks.len() != breaks
            || context.continues.len() != continues
            || (qualified.conditional.is_none() && self.block != current_carrier.active_body())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.finish_continues(&context, context.header)?;
        if qualified.conditional.is_some() {
            self.validate_conditional_map_body(&context, span)?;
        } else if continues == 1 {
            current_carrier.validate_backedge(self, source_slot, cursor_slot, span)?;
        } else if matches!(qualified.body_exit, UnitIterationExitKind::Return(_)) {
            current_carrier.validate_return(self, span)?;
        }
        self.validate_map_stack_backedges(&parent_header, &context, span)?;
        self.restore_loop_state(&false_state);
        self.restore_map_stack(&parent_false, span)?;
        let iteration = context
            .iteration
            .as_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let IterationState::Map(state) = &mut iteration.state else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        self.restore_conditional_map_exhaustion(state, span)?;
        state.phase = MapPhase::Terminal {
            point: UnitDropPoint::LoopExit(statement_id),
            finished: false,
            source_ended: false,
            owner_dropped: false,
        };
        self.loops.push(context);
        self.emit_drops(UnitDropPoint::LoopExit(statement_id))?;
        self.pending_operands.truncate(retained_start);
        let exit = self.loop_state();
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.validate_conditional_map_complete(&context, span)?;
        let mut exits = context
            .breaks
            .into_iter()
            .map(branch_exit)
            .collect::<Vec<_>>();
        exits.push(exit);
        let parent_exits = self.map_conditional_exits(&parent_checkpoint, &exits, span)?;
        self.merge_loop_exits(exits, span)?;
        self.finish_map_conditional_merge(
            &parent_checkpoint,
            parent_exits,
            LoweredValue::Unit,
            span,
        )?;
        self.finish_local_map_loop(qualified, carrier, false_block, retained_start, span)?;
        Ok(LoweredValue::Unit)
    }

    fn map_iteration_value(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                operation,
                vec![EntityType::Value(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        match results.as_slice() {
            [EntityId::Value(value)] => Ok(*value),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }
    fn map_pending_source(&self, slot: usize, span: Span) -> Result<LoanId, LoweringError> {
        match self.pending_operands.get(slot) {
            Some(EntityId::Loan(loan)) => Ok(*loan),
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }
    fn map_pending_cursor(
        &self,
        slot: usize,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        match self.pending_operands.get(slot).copied() {
            Some(EntityId::Value(value))
                if self
                    .function
                    .entity(EntityId::Value(value))
                    .map(|data| data.ty)
                    == Some(EntityType::Value(ty)) =>
            {
                Ok(value)
            }
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }
    pub(super) fn advance_map_iteration(
        &mut self,
        state: MapState,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !matches!(
            state.phase,
            MapPhase::Body {
                key_live: false,
                value_live: false
            }
        ) || self.borrow_bindings.contains_key(&state.key_binding)
            || self.borrow_bindings.contains_key(&state.value_binding)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_map_state_current(state, state.body_block(), span)?;
        let source = self.map_pending_source(state.source_slot, span)?;
        let cursor = self.map_pending_cursor(state.cursor_slot, state.cursor_type, span)?;
        let next = self.map_iteration_value(
            Operation::MapIterationNext { source, cursor },
            state.cursor_type,
            span,
        )?;
        self.record_conditional_map_next(state, next, span)?;
        self.pending_operands[state.cursor_slot] = EntityId::Value(next);
        Ok(())
    }
}

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn record_map_return_delivery(
        &mut self,
        expression: UnitExpressionId,
        result: LoweredValue,
        span: Span,
    ) -> Result<(), LoweringError> {
        let boundary = self
            .function
            .block(self.block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            .instructions
            .len();
        self.record_conditional_return_delivery(expression, result, boundary, span)?;
        for context in &mut self.loops {
            let Some(iteration) = &mut context.iteration else {
                continue;
            };
            let IterationState::Map(state) = &mut iteration.state else {
                continue;
            };
            if state.conditional.is_none()
                && state.carrier.body_point == UnitDropPoint::ControlTransfer(expression)
                && state
                    .carrier
                    .return_delivery
                    .replace((result, self.block, boundary))
                    .is_some()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }
}
