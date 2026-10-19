//! Local birth and the single exact post-provider size/drop obligation.
mod completed;
mod formed_birth;
use super::*;
pub(super) use formed_birth::validate_formed_birth;
use lang_frontend::ownership_checking::UnitDropFact;

pub(super) fn validate_birth(
    lowerer: &UnitExpressionLowerer<'_>,
    root: UnitSymbolId,
    initializer: UnitExpressionId,
    current: EntityId,
    span: Span,
) -> Result<EntityId, LoweringError> {
    validate_call_owner(lowerer, root, (initializer, true), current, span)
}

pub(super) fn validate_other_epoch_birth(
    lowerer: &UnitExpressionLowerer<'_>,
    root: UnitSymbolId,
    initializer: UnitExpressionId,
    current: EntityId,
    span: Span,
) -> Result<EntityId, LoweringError> {
    validate_call_owner(lowerer, root, (initializer, false), current, span)
}

fn validate_call_owner(
    lowerer: &UnitExpressionLowerer<'_>,
    root: UnitSymbolId,
    birth: (UnitExpressionId, bool),
    current: EntityId,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let (initializer, provider) = birth;
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    if root.source_unit() != lowerer.source_unit
        || initializer.source_unit() != lowerer.source_unit
        || !lowerer.owned.formed_owned_locals().contains(&root)
        || lowerer.borrow_bindings.contains_key(&root)
        || !matches!(current, EntityId::Value(_))
    {
        return Err(bad());
    }
    // A different epoch is not a provider. Reuse the ordinary direct-call
    // route, and report other unimplemented birth kinds before looking for an
    // instruction belonging to that route.
    if !provider {
        let call = lowerer.typed.call(initializer).ok_or_else(bad)?;
        if !matches!(
            call.target(),
            lang_frontend::type_checking::UnitCallTarget::Declaration(_)
                | lang_frontend::type_checking::UnitCallTarget::Symbol(_)
        ) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
    }
    let node = lowerer
        .parsed
        .ast()
        .expressions()
        .get(initializer.expression())
        .map_err(|_| bad())?;
    let births = lowerer
        .function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(node.span())
                && matches!(instruction.operation, Operation::DirectCall { .. })
        })
        .collect::<Vec<_>>();
    let [birth] = births.as_slice() else {
        return Err(bad());
    };
    let [anchor @ EntityId::Value(_)] = birth.results.as_slice() else {
        return Err(bad());
    };
    source::validate_ordinary_birth(lowerer, initializer, *anchor, span)?;
    transport::from_anchor(lowerer.function, current, *anchor, span)?;
    if lowerer.bindings.iter().any(|(other, value)| {
        *other != root
            && matches!(value, LoweredValue::Value(v)
            if EntityId::Value(*v) == current || EntityId::Value(*v) == *anchor)
    }) || lowerer
        .temporaries
        .values()
        .any(|value| EntityId::Value(*value) == current || EntityId::Value(*value) == *anchor)
        || lowerer
            .pending_operands
            .iter()
            .any(|entity| *entity == current || *entity == *anchor)
    {
        return Err(bad());
    }
    Ok(*anchor)
}

impl UnitExpressionLowerer<'_> {
    pub(in super::super::super) fn finish_local_map_continuation(
        &self,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.map_local_continuation.is_some() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn finish_local_map_loop(
        &mut self,
        qualified: map_iteration::QualifiedMap,
        carrier: MapCarrier,
        false_block: BlockId,
        retained_start: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        let SourceIdentity::Local { .. } = qualified.source else {
            return Ok(());
        };
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        if self.block != false_block
            || self.pending_operands.len() != retained_start
            || !carrier.owned
            || qualified.source_drop.is_some() == qualified.late_drop.is_some()
        {
            return Err(bad());
        }
        carrier.validate_local_exhaustion(self, span)?;
        let block = self.function.block(false_block).ok_or_else(bad)?;
        for id in &block.instructions {
            let instruction = self.function.instruction(*id).ok_or_else(bad)?;
            if matches!(
                instruction.operation,
                Operation::MapIterationKeyLoan { .. }
                    | Operation::MapIterationValueLoan { .. }
                    | Operation::MapIterationNext { .. }
            ) {
                return Err(bad());
            }
        }
        let Some(drop) = qualified.late_drop else {
            if qualified.source.is_registered(self) {
                return Err(bad());
            }
            return Ok(());
        };
        let current = qualified.source.current(self, span)?;
        transport::from_anchor(self.function, current, carrier.anchor, span)?;
        if !matches!(current, EntityId::Value(_))
            || self.map_local_continuation.is_some()
            || self
                .temporaries
                .values()
                .any(|value| EntityId::Value(*value) == current)
            || self.pending_operands.contains(&current)
        {
            return Err(bad());
        }
        self.map_local_continuation = Some(map_iteration::LocalContinuation {
            source: qualified.source,
            drop,
            birth: carrier.anchor,
            current,
            block: self.block,
            read: None,
        });
        Ok(())
    }

    pub(in super::super::super) fn check_local_map_size(
        &self,
        expression: UnitExpressionId,
        owner: EntityId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(proof) = self.map_local_continuation else {
            return Ok(());
        };
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        if proof.drop.point() != UnitDropPoint::CallReturn(expression)
            || proof.read.is_some()
            || self.block != proof.block
            || owner != proof.current
            || proof.source.current(self, span)? != owner
            || !matches!(owner, EntityId::Value(_))
        {
            return Err(bad());
        }
        transport::from_anchor(self.function, owner, proof.birth, span)?;
        if self
            .temporaries
            .values()
            .any(|value| EntityId::Value(*value) == owner)
            || self.pending_operands.contains(&owner)
        {
            return Err(bad());
        }
        Ok(())
    }

    pub(in super::super::super) fn record_local_map_size(
        &mut self,
        expression: UnitExpressionId,
        owner: EntityId,
        result: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.map_local_continuation.is_none() {
            return Ok(());
        }
        self.check_local_map_size(expression, owner, span)?;
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let instruction = carrier::instruction(self.function, EntityId::Value(result), span)?;
        if instruction.block != self.block
            || instruction.operation != (Operation::MapSize { owner })
            || instruction.origin != Origin::Source(span)
        {
            return Err(bad());
        }
        self.map_local_continuation.as_mut().ok_or_else(bad)?.read = Some(result);
        Ok(())
    }

    pub(in super::super::super) fn emit_local_map_late_drop(
        &mut self,
        fact: UnitDropFact,
    ) -> Result<bool, LoweringError> {
        let Some(proof) = self.map_local_continuation else {
            return Ok(false);
        };
        if fact.target() != proof.source.drop_target() {
            return Ok(false);
        }
        let span = fact.value_origin();
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let Some(read) = proof.read else {
            return Err(bad());
        };
        let EntityId::Value(owner) = proof.current else {
            return Err(bad());
        };
        let size = carrier::instruction(self.function, EntityId::Value(read), span)?;
        if fact != proof.drop
            || self.block != proof.block
            || proof.source.current(self, span)? != proof.current
            || size.block != self.block
            || size.operation
                != (Operation::MapSize {
                    owner: proof.current,
                })
            || self
                .function
                .block(self.block)
                .and_then(|block| block.instructions.last())
                .copied()
                != Some(size.id)
        {
            return Err(bad());
        }
        transport::from_anchor(self.function, proof.current, proof.birth, span)?;
        self.emit_drop_fact(fact)?;
        let last = self
            .function
            .block(self.block)
            .and_then(|block| block.instructions.last())
            .and_then(|id| self.function.instruction(*id))
            .ok_or_else(bad)?;
        if last.operation != (Operation::Drop { owner })
            || last.origin != Origin::Source(span)
            || proof.source.is_registered(self)
            || self.temporaries.values().any(|value| *value == owner)
            || self
                .bindings
                .values()
                .any(|value| *value == LoweredValue::Value(owner))
            || self.pending_operands.contains(&proof.current)
        {
            return Err(bad());
        }
        self.map_local_continuation = None;
        Ok(true)
    }

    pub(in super::super::super) fn finish_local_map_size(
        &self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self
            .map_local_continuation
            .is_some_and(|proof| proof.drop.point() == UnitDropPoint::CallReturn(expression))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }
}
