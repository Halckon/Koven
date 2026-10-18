//! Exact source storage identity, shared by the bounded provider and its carrier.
use super::*;
use lang_frontend::type_checking::CallableTarget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceIdentity {
    Parameter(SymbolId),
    Local {
        symbol: SymbolId,
        initializer: ExpressionId,
    },
    /// Current owner follows an original Call through normal binding/CFG transport.
    Formed {
        symbol: SymbolId,
        producer: ExpressionId,
    },
    Temporary(ExpressionId),
}

impl SourceIdentity {
    pub(super) fn matches_slot(self, slot: &LinearBindingSlot) -> bool {
        match self {
            Self::Parameter(symbol) | Self::Local { symbol, .. } | Self::Formed { symbol, .. } => {
                slot.symbol == Some(symbol)
            }
            Self::Temporary(expression) => {
                slot.symbol.is_none() && slot.temporaries.as_slice() == [expression.index()]
            }
        }
    }

    pub(super) fn current_owner(self, lowerer: &ExpressionLowerer<'_>) -> Option<ValueId> {
        match self {
            Self::Parameter(symbol) | Self::Local { symbol, .. } | Self::Formed { symbol, .. } => {
                match lowerer.bindings.get(&symbol) {
                    Some(LoweredValue::Value(owner)) => Some(*owner),
                    _ => None,
                }
            }
            Self::Temporary(expression) => lowerer.temporaries.get(&expression.index()).copied(),
        }
    }

    pub(super) fn take_owner(
        self,
        lowerer: &mut ExpressionLowerer<'_>,
        owner: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.current_owner(lowerer) != Some(owner) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        match self {
            Self::Parameter(symbol) | Self::Local { symbol, .. } | Self::Formed { symbol, .. } => {
                lowerer.bindings.remove(&symbol);
            }
            Self::Temporary(expression) => {
                // The admitted source is a direct Call, so there are no transparent
                // aliases to erase and no unrelated outer owner may be removed.
                if lowerer
                    .temporaries
                    .iter()
                    .any(|(&key, &value)| key != expression.index() && value == owner)
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                lowerer.temporaries.remove(&expression.index());
            }
        }
        Ok(())
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn map_iteration_seed(
        &mut self,
        source: SourceIdentity,
        container: SsaTypeId,
        origin: Span,
        _nested: Option<nested::Pair>,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        match source {
            SourceIdentity::Parameter(symbol) => {
                let seed = *self
                    .map_iteration_state
                    .entries
                    .get(&symbol)
                    .ok_or_else(missing)?;
                let entry = self.function.entry_block().ok_or_else(missing)?;
                if !matches!(self.function.entity(seed).map(|entity| entity.definition),
                    Some(Definition::BlockParameter { block, .. }) if block == entry)
                {
                    return Err(missing());
                }
                let current = match seed {
                    EntityId::Value(_) => match self.bindings.get(&symbol) {
                        Some(LoweredValue::Value(owner)) => EntityId::Value(*owner),
                        _ => return Err(missing()),
                    },
                    EntityId::Loan(_) => {
                        EntityId::Loan(*self.borrow_bindings.get(&symbol).ok_or_else(missing)?)
                    }
                    _ => return Err(missing()),
                };
                nested::transport(self.function, current, seed, span)?;
                Ok(current)
            }
            SourceIdentity::Local {
                symbol,
                initializer,
            } => self.local_map_seed(symbol, initializer, container, span),
            SourceIdentity::Formed { symbol, producer } => {
                let formation = formed::formation(
                    self.parsed,
                    self.names,
                    self.typed,
                    self.owned,
                    symbol,
                    self.typed.expression_type(producer).ok_or_else(missing)?,
                    span,
                )?;
                if formation.producer != producer {
                    return Err(missing());
                }
                let anchor = self.formed_map_anchor(&formation, None, span)?;
                let instruction = carrier::definition(self.function, EntityId::Value(anchor))
                    .ok_or_else(missing)?;
                let call = self.typed.call(producer).ok_or_else(missing)?;
                let CallableTarget::Source(target) = call.target() else {
                    return Err(missing());
                };
                let callee = *self
                    .function_ids
                    .get(&FunctionInstanceKey::new(target, Vec::new()))
                    .ok_or_else(missing)?;
                if instruction.operation
                    != (Operation::DirectCall {
                        callee,
                        receiver: None,
                        arguments: Vec::new(),
                    })
                    || self.function.entity(EntityId::Value(anchor)).map(|e| e.ty)
                        != Some(EntityType::Value(container))
                {
                    return Err(missing());
                }
                let owner = source.current_owner(self).ok_or_else(missing)?;
                Ok(EntityId::Value(owner))
            }
            SourceIdentity::Temporary(expression) => {
                let call = self.typed.call(expression).ok_or_else(missing)?;
                let CallableTarget::Source(symbol) = call.target() else {
                    return Err(missing());
                };
                let instance = FunctionInstanceKey::new(symbol, Vec::new());
                let expected = *self.function_ids.get(&instance).ok_or_else(missing)?;
                let LoweredValue::Value(owner) = self.lower(expression)? else {
                    return Err(missing());
                };
                // Ordinary argument lowering owns its CFG and loan cleanup. The
                // provider starts only after that call has delivered this owner.
                let preheader = self.block;
                let seed = EntityId::Value(owner);
                let instruction = carrier::definition(self.function, seed).ok_or_else(missing)?;
                if instruction.block != preheader
                    || instruction.origin != Origin::Source(origin)
                    || instruction.results != [seed]
                    // The ordinary call emitter already consumes the selected
                    // argument mapping and closes its argument loans. This proof
                    // anchors that emitted result, without evaluating operands again.
                    || !matches!(&instruction.operation, Operation::DirectCall {
                        callee, receiver: None, arguments,
                    } if *callee == expected && arguments.len() == call.arguments().len())
                    || self.function.entity(seed).map(|entity| entity.ty)
                        != Some(EntityType::Value(container))
                    || source.current_owner(self) != Some(owner)
                    || self.temporaries.len() != 1
                    || !self.pending_call_loans.is_empty()
                    || !self.capture_loans.is_empty()
                    || !self.result_source_loans.is_empty()
                {
                    return Err(missing());
                }
                Ok(seed)
            }
        }
    }
}

impl ExpressionLowerer<'_> {
    pub(in super::super) fn validate_pure_map_parameter_drop(
        &self,
        fact: lang_frontend::ownership_checking::DropFact,
        owner: ValueId,
    ) -> Result<bool, LoweringError> {
        use lang_frontend::ownership_checking::DropTarget;
        let DropTarget::Named(root) = fact.target() else {
            return Ok(false);
        };
        let Some(&anchor) = self.map_iteration_state.entries.get(&root) else {
            return Ok(false);
        };
        let candidate = self.typed.map_iterations().iter().any(|descriptor| {
            self.typed.is_resource_type(descriptor.source_type()) == Some(false)
                && self
                    .owned
                    .map_iteration(descriptor.statement())
                    .is_some_and(|plan| {
                        matches!(plan.source(), Some(LoanTarget::Place(place))
                        if place.is_root() && place.root() == root)
                    })
        });
        if !candidate
            || !source_owner::supported_drop(self.parsed, self.names, self.typed, self.owned, fact)
        {
            return Ok(false);
        }
        let bad = || error(LoweringErrorKind::MissingFact, fact.value_origin());
        if !matches!(anchor, EntityId::Value(_))
            || self.borrow_bindings.contains_key(&root)
            || self.temporaries.values().any(|&v| v == owner)
            || self
                .bindings
                .values()
                .any(|v| *v == LoweredValue::Value(owner))
        {
            return Err(bad());
        }
        nested::transport(
            self.function,
            EntityId::Value(owner),
            anchor,
            fact.value_origin(),
        )?;
        Ok(true)
    }
}
