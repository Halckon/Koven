//! Exact source storage identity, shared by the bounded provider and its carrier.
use super::*;
use lang_frontend::type_checking::CallableTarget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceIdentity {
    Parameter(SymbolId),
    Temporary(ExpressionId),
}

impl SourceIdentity {
    pub(super) fn matches_slot(self, slot: &LinearBindingSlot) -> bool {
        match self {
            Self::Parameter(symbol) => slot.symbol == Some(symbol),
            Self::Temporary(expression) => {
                slot.symbol.is_none() && slot.temporaries.as_slice() == [expression.index()]
            }
        }
    }

    pub(super) fn current_owner(self, lowerer: &ExpressionLowerer<'_>) -> Option<ValueId> {
        match self {
            Self::Parameter(symbol) => match lowerer.bindings.get(&symbol) {
                Some(LoweredValue::Value(owner)) => Some(*owner),
                _ => None,
            },
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
            Self::Parameter(symbol) => {
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
                Ok(seed)
            }
            SourceIdentity::Temporary(expression) => {
                let call = self.typed.call(expression).ok_or_else(missing)?;
                let CallableTarget::Source(symbol) = call.target() else {
                    return Err(missing());
                };
                let instance = FunctionInstanceKey::new(symbol, Vec::new());
                let expected = *self.function_ids.get(&instance).ok_or_else(missing)?;
                let preheader = self.block;
                let LoweredValue::Value(owner) = self.lower(expression)? else {
                    return Err(missing());
                };
                let seed = EntityId::Value(owner);
                let instruction = carrier::definition(self.function, seed).ok_or_else(missing)?;
                if self.block != preheader
                    || instruction.block != preheader
                    || instruction.origin != Origin::Source(origin)
                    || instruction.results != [seed]
                    || instruction.operation
                        != (Operation::DirectCall {
                            callee: expected,
                            receiver: None,
                            arguments: Vec::new(),
                        })
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
