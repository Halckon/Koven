//! Integer Range loops carry endpoint Values, independently of borrowed providers.
use super::{
    iteration::{IterationContext, IterationState},
    *,
};
use crate::ssa::model::{
    BinaryOperator, ComparisonOperator, EntityId, EntityType, Operation, ScalarConstant, SsaTypeId,
    ValueId,
};
use lang_frontend::{
    ownership_checking::{UnitIterationOwnershipPlan, UnitIterationSourceAccess},
    type_checking::{BuiltinType, IterationProvider, UnitSequentialIterationBinding},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_numeric_for(
        &mut self,
        plan: &UnitIterationOwnershipPlan,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = plan.descriptor();
        let endpoint = match descriptor.provider() {
            IterationProvider::NumericRange(endpoint @ (BuiltinType::Int | BuiltinType::Long)) => {
                endpoint
            }
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let endpoint_type = self.iteration_type(descriptor.element_type(), span)?;
        if plan.source_access() != UnitIterationSourceAccess::Owned
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
        // Evaluate the real source once, including its AfterExpression cleanup, before
        // acquiring this provider. Copyable projections detach the iteration snapshot.
        let source = match self.lower(descriptor.source().expression())? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if self
            .function
            .entity(EntityId::Value(source))
            .map(|value| value.ty)
            != Some(EntityType::Value(range_type))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let first = self.numeric_iteration_value(
            Operation::AggregateProject {
                aggregate: source,
                field: 0,
            },
            endpoint_type,
            span,
        )?;
        let last = self.numeric_iteration_value(
            Operation::AggregateProject {
                aggregate: source,
                field: 1,
            },
            endpoint_type,
            span,
        )?;
        let empty = self.numeric_iteration_value(
            Operation::AggregateProject {
                aggregate: source,
                field: 2,
            },
            boolean,
            span,
        )?;
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
            let current = self.numeric_pending_value(retained_start, endpoint_type, span)?;
            let element = self.numeric_iteration_value(
                Operation::Copy { source: current },
                endpoint_type,
                span,
            )?;
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
        self.finish_numeric_continues(&context, &baseline, exhausted, span)?;

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

    fn finish_numeric_continues(
        &mut self,
        context: &LoopContext,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        exhausted: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if context.continues.is_empty() {
            return Ok(());
        }
        let Some(IterationContext {
            state:
                IterationState::Numeric {
                    current,
                    last,
                    endpoint_type,
                },
            ..
        }) = context.iteration.as_ref()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let (current_slot, last_slot, endpoint_type) = (*current, *last, *endpoint_type);
        let continuation =
            self.add_carried_control_block(&context.carried, &context.loans, span)?;
        self.finish_continues(context, continuation)?;
        self.block = continuation;
        self.current_receiver = context.entry_receiver;
        self.consumed_receiver = context.entry_consumed_receiver;
        self.bindings = self.rebind_loop(baseline, continuation, context, span)?;
        self.closure_bindings = context.entry_closure_bindings.clone();
        let current = self.numeric_pending_value(current_slot, endpoint_type, span)?;
        let last = self.numeric_pending_value(last_slot, endpoint_type, span)?;
        let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
        let terminal = self.numeric_iteration_value(
            Operation::Compare {
                operator: ComparisonOperator::Equal,
                left: current,
                right: last,
            },
            boolean,
            span,
        )?;
        let increment = self.add_carried_control_block(&context.carried, &context.loans, span)?;
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
        self.bindings = self.rebind_loop(baseline, increment, context, span)?;
        let current = self.numeric_pending_value(current_slot, endpoint_type, span)?;
        let one = self.numeric_iteration_value(
            Operation::Constant(ScalarConstant::Integer(1)),
            endpoint_type,
            span,
        )?;
        // A nonempty ascending range reaches this edge only when current < last.
        // The endpoint-width increment therefore cannot overflow, including MAX..MAX.
        let next = self.numeric_iteration_value(
            Operation::Binary {
                operator: BinaryOperator::Add,
                left: current,
                right: one,
            },
            endpoint_type,
            span,
        )?;
        self.pending_operands[current_slot] = EntityId::Value(next);
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
        Ok(())
    }

    pub(super) fn numeric_pending_value(
        &self,
        index: usize,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        match self.pending_operands.get(index).copied() {
            Some(EntityId::Value(value))
                if self
                    .function
                    .entity(EntityId::Value(value))
                    .map(|value| value.ty)
                    == Some(EntityType::Value(ty)) =>
            {
                Ok(value)
            }
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }

    fn numeric_iteration_value(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        match self.append_scalar(operation, ty, span)? {
            LoweredValue::Value(value) => Ok(value),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }
}
