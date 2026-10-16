//! Owned Copyable cursor state, independent of the shared container provider.
use super::*;
use crate::ssa::model::{BinaryOperator, ComparisonOperator, ScalarConstant};
use lang_frontend::{
    ownership_checking::{IterationExitKind, IterationOwnershipPlan},
    type_checking::{BuiltinType, IterationProvider, SequentialIterationBinding},
};

pub(in super::super) struct NumericLoopData {
    pub(super) plan: IterationOwnershipPlan,
    current: ValueId,
    last: ValueId,
    endpoint: SsaTypeId,
    exhaustion: BlockId,
    baseline: BTreeMap<SymbolId, LoweredValue>,
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_numeric_for(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        span: Span,
        plan: IterationOwnershipPlan,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = plan.descriptor();
        if descriptor.statement() != statement {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let endpoint = self.resolve_type(descriptor.element_type(), span)?;
        let endpoint = *self
            .type_ids
            .get(&endpoint)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let endpoint_builtin = match descriptor.provider() {
            IterationProvider::NumericRange(value @ (BuiltinType::Int | BuiltinType::Long)) => {
                value
            }
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        if endpoint != self.ssa_builtin(endpoint_builtin, span)? {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let range_type = self.resolve_type(descriptor.source_type(), span)?;
        let range_type = *self
            .type_ids
            .get(&range_type)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
        let snapshot = match self.lower(source)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        if self
            .function
            .entity(EntityId::Value(snapshot))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(range_type))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let first = self.numeric_projection(snapshot, 0, endpoint, span)?;
        let last = self.numeric_projection(snapshot, 1, endpoint, span)?;
        let empty = self.numeric_projection(snapshot, 2, boolean, span)?;
        let baseline = self.bindings.clone();
        let mut carried = self.linear_binding_slots(&baseline, span)?;
        // All mutable Copyable bindings are loop-carried, just as for while/container loops.
        for (&symbol, &binding) in &baseline {
            if let LoweredValue::Value(value) = binding
                && !carried.slots.iter().any(|slot| slot.symbol == Some(symbol))
            {
                let source = EntityId::Value(value);
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                carried
                    .slots
                    .push(LinearBindingSlot::new(Some(symbol), source, ty));
            }
        }
        let mut body_types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        body_types.push(EntityType::Value(endpoint));
        let header = self
            .function
            .add_block(body_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let exhaustion = self.add_linear_binding_block(&carried, span)?;
        let mut body_args = carried
            .slots
            .iter()
            .map(|slot| slot.source)
            .collect::<Vec<_>>();
        body_args.push(EntityId::Value(first));
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: empty,
                    when_true: loop_edge(exhaustion, &carried),
                    when_false: Edge {
                        target: header,
                        arguments: body_args,
                    },
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.block = header;
        self.bindings = self.rebind_linear_bindings(&baseline, header, &carried, span)?;
        let current = match self
            .function
            .block(header)
            .and_then(|block| block.parameters.last())
            .copied()
        {
            Some(EntityId::Value(value)) => value,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        match descriptor.binding() {
            SequentialIterationBinding::Name(symbol) => {
                self.bindings.insert(*symbol, LoweredValue::Value(current));
            }
            SequentialIterationBinding::Discard => {}
            SequentialIterationBinding::Destructure(_) => {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        self.loops.push(LoopContext {
            entry_views: self.non_null_bindings.clone(),
            header,
            carried: carried.clone(),
            continues: Vec::new(),
            breaks: Vec::new(),
            for_loop: None,
            numeric_loop: Some(NumericLoopData {
                plan: plan.clone(),
                current,
                last,
                endpoint,
                exhaustion,
                baseline: baseline.clone(),
            }),
        });
        if self.lower_statement(body)? != LoweredValue::Diverged {
            let exit = plan
                .exits()
                .iter()
                .find(|exit| matches!(exit.kind(), IterationExitKind::Fallthrough))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.emit_iteration_exit_plan(exit, span)?;
            self.finish_numeric_iteration(span)?;
        }
        self.block = exhaustion;
        self.bindings = self.rebind_linear_bindings(&baseline, exhaustion, &carried, span)?;
        let exit = plan
            .exits()
            .iter()
            .find(|exit| matches!(exit.kind(), IterationExitKind::Exhaustion))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.emit_iteration_exit_plan(exit, span)?;
        let context = self
            .loops
            .pop()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut exits = context.breaks;
        exits.push(self.loop_exit());
        // Numeric snapshots are pure values; the normal merge still preserves outer loans/owners.
        self.merge_exits(exits, &baseline, span)
    }

    fn numeric_projection(
        &mut self,
        aggregate: ValueId,
        field: usize,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        self.numeric_value(Operation::AggregateProject { aggregate, field }, ty, span)
    }

    fn numeric_value(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
        match results.as_slice() {
            [EntityId::Value(value)] => Ok(*value),
            _ => Err(error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    pub(super) fn lower_numeric_break(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let plan = self
            .loops
            .last()
            .and_then(|context| context.numeric_loop.as_ref())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .plan
            .clone();
        let exit = plan
            .exits()
            .iter()
            .find(|exit| matches!(exit.kind(), IterationExitKind::Break(id) if id == expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.emit_iteration_exit_plan(exit, span)?;
        self.lower_break(span)
    }

    pub(super) fn lower_numeric_continue(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let plan = self
            .loops
            .last()
            .and_then(|context| context.numeric_loop.as_ref())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .plan
            .clone();
        let exit = plan
            .exits()
            .iter()
            .find(|exit| matches!(exit.kind(), IterationExitKind::Continue(id) if id == expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.emit_iteration_exit_plan(exit, span)?;
        self.finish_numeric_iteration(span)?;
        Ok(LoweredValue::Diverged)
    }

    fn finish_numeric_iteration(&mut self, span: Span) -> Result<(), LoweringError> {
        let context = self
            .loops
            .last()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let data = context
            .numeric_loop
            .as_ref()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (current, last, endpoint, exhaustion, baseline, header, carried) = (
            data.current,
            data.last,
            data.endpoint,
            data.exhaustion,
            data.baseline.clone(),
            context.header,
            context.carried.clone(),
        );
        let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
        let terminal = self.numeric_value(
            Operation::Compare {
                operator: ComparisonOperator::Equal,
                left: current,
                right: last,
            },
            boolean,
            span,
        )?;
        let increment = self.add_linear_binding_block(&carried, span)?;
        let state = self.loop_exit();
        let args = carried
            .slots
            .iter()
            .map(|slot| loop_slot_entity(slot, &state, span))
            .collect::<Result<Vec<_>, _>>()?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: terminal,
                    when_true: Edge {
                        target: exhaustion,
                        arguments: args.clone(),
                    },
                    when_false: Edge {
                        target: increment,
                        arguments: args,
                    },
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.block = increment;
        self.bindings = self.rebind_linear_bindings(&baseline, increment, &carried, span)?;
        // This block is reachable only when current != last; ascending current cannot overflow.
        let one = self.numeric_value(
            Operation::Constant(ScalarConstant::Integer(1)),
            endpoint,
            span,
        )?;
        let next = self.numeric_value(
            Operation::Binary {
                operator: BinaryOperator::Add,
                left: current,
                right: one,
            },
            endpoint,
            span,
        )?;
        let state = self.loop_exit();
        let mut args = carried
            .slots
            .iter()
            .map(|slot| loop_slot_entity(slot, &state, span))
            .collect::<Result<Vec<_>, _>>()?;
        args.push(EntityId::Value(next));
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(Edge {
                    target: header,
                    arguments: args,
                }),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }
}
