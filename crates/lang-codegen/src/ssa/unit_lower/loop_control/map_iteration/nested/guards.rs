//! A guard is the actual constructor result, carried by ordinary owner bindings.
use super::*;
use lang_frontend::ownership_checking::UnitDropFact;

impl UnitExpressionLowerer<'_> {
    pub(in crate::ssa::unit_lower) fn nested_guard_current(
        &self,
        guard: map_iteration::QualifiedGuard,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        match self.bindings.get(&guard.symbol) {
            Some(LoweredValue::Value(owner)) => Ok(*owner),
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }

    pub(in crate::ssa::unit_lower) fn nested_guard_birth(
        &self,
        guard: map_iteration::QualifiedGuard,
        block: BlockId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let owner = self.nested_guard_current(guard, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(guard.initializer.expression())
            .map_err(|_| bad())?;
        let descriptor = self.typed.construction(guard.initializer).ok_or_else(bad)?;
        let ty = self.expression_ssa_type(guard.initializer.expression(), span)?;
        let birth = carrier::instruction(self.function, EntityId::Value(owner), span)?;
        let Operation::HeapAllocate {
            owner: actual_type,
            payload,
        } = birth.operation
        else {
            return Err(bad());
        };
        let payload = carrier::instruction(self.function, EntityId::Value(payload), span)?;
        let aggregate = self.heap_payloads.get(&ty).copied().ok_or_else(bad)?;
        if guard.initializer.source_unit() != self.source_unit
            || guard.symbol.source_unit() != self.source_unit
            || descriptor.expression() != guard.initializer
            || descriptor.result_type() != self.typed.symbol_type(guard.symbol).ok_or_else(bad)?
            || birth.block != block
            || birth.block != self.block
            || actual_type != ty
            || birth.origin != Origin::Source(node.span())
            || payload.block != block
            || payload.origin != birth.origin
            || payload.operation
                != (Operation::AggregateConstruct {
                    aggregate,
                    fields: Vec::new(),
                })
            || self
                .function
                .entity(EntityId::Value(owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(ty))
        {
            return Err(bad());
        }
        let births = self
            .function
            .instructions
            .iter()
            .filter(|instruction| {
                instruction.origin == Origin::Source(node.span())
                    && matches!(instruction.operation, Operation::HeapAllocate { .. })
            })
            .collect::<Vec<_>>();
        if !matches!(births.as_slice(), [actual] if actual.id == birth.id) {
            return Err(bad());
        }
        Ok(owner)
    }

    pub(in crate::ssa::unit_lower) fn emit_nested_guard_drop(
        &mut self,
        fact: UnitDropFact,
    ) -> Result<bool, LoweringError> {
        let found = self
            .loops
            .iter()
            .filter_map(|context| context.iteration.as_ref())
            .find_map(|iteration| {
                let IterationState::Map(state) = &iteration.state else {
                    return None;
                };
                let pair = state.carrier.nested?;
                let tail = pair.tail?;
                let index = usize::from(iteration.statement == pair.inner);
                let guard = tail.guards[index];
                (fact.target()
                    == lang_frontend::ownership_checking::UnitDropTarget::Named(guard.symbol))
                .then_some((**state, guard))
            });
        let Some((state, guard)) = found else {
            return Ok(false);
        };
        let span = fact.value_origin();
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let event = state.nested.event(fact.point()).ok_or_else(bad)?;
        let Some((symbol, owner)) = event.guard else {
            return Err(bad());
        };
        if event.end.is_some()
            || event.block != self.block
            || symbol != guard.symbol
            || fact.value_origin() != guard.origin
            || self.nested_guard_current(guard, span)? != owner
            || !matches!(
                state.phase,
                MapPhase::Body {
                    key_live: true,
                    value_live: true
                }
            )
        {
            return Err(bad());
        }
        let anchor = state.nested.guard_birth.ok_or_else(bad)?;
        transport::from_anchor(
            self.function,
            EntityId::Value(owner),
            EntityId::Value(anchor),
            span,
        )?;
        self.emit_drop_fact(fact)?;
        if self.bindings.contains_key(&guard.symbol) {
            return Err(bad());
        }
        let last = self
            .function
            .block(self.block)
            .and_then(|block| block.instructions.last())
            .and_then(|id| self.function.instruction(*id))
            .ok_or_else(bad)?;
        if last.operation != (Operation::Drop { owner })
            || last.origin != Origin::Source(guard.origin)
        {
            return Err(bad());
        }
        Ok(true)
    }
}
