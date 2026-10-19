//! Exact source storage identity, shared by the bounded provider and its carrier.
use super::*;
use lang_frontend::type_checking::CallableTarget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceIdentity {
    Parameter(SymbolId),
    BorrowedBinding(borrowed::Proof),
    Local {
        symbol: SymbolId,
        initializer: ExpressionId,
    },
    /// Current owner follows an original Call through normal binding/CFG transport.
    Formed {
        symbol: SymbolId,
        birth: crate::ssa::map_source::Birth<SymbolId>,
    },
    Temporary(ExpressionId),
}

impl SourceIdentity {
    pub(super) fn matches_slot(self, slot: &LinearBindingSlot) -> bool {
        match self {
            Self::BorrowedBinding(_) => false,
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
            Self::BorrowedBinding(_) => None,
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
            Self::BorrowedBinding(_) => return Err(error(LoweringErrorKind::MissingFact, span)),
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
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        match source {
            SourceIdentity::BorrowedBinding(proof) => self.borrowed_map_seed(proof, span),
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
            SourceIdentity::Formed { symbol, birth } => {
                let formation = formed::formation(
                    self.parsed,
                    self.names,
                    self.typed,
                    self.owned,
                    symbol,
                    self.typed.symbol_type(symbol).ok_or_else(missing)?,
                    span,
                )?;
                if formation.birth != birth {
                    return Err(missing());
                }
                self.formed_map_anchor(&formation, None, span)?;
                let owner = source.current_owner(self).ok_or_else(missing)?;
                if self.function.entity(EntityId::Value(owner)).map(|e| e.ty)
                    != Some(EntityType::Value(container))
                {
                    return Err(missing());
                }
                let owner = source.current_owner(self).ok_or_else(missing)?;
                Ok(EntityId::Value(owner))
            }
            SourceIdentity::Temporary(expression) => {
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
                    || self.map_call_birth(expression, span)? != owner
                    || self.function.entity(seed).map(|entity| entity.ty)
                        != Some(EntityType::Value(container))
                    || source.current_owner(self) != Some(owner)
                    || self.temporaries.iter().any(|(&key, &value)| {
                        key != expression.index() && !self.active_map_temporary(key, value)
                    })
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
    /// Anchor the result already emitted by ordinary argument lowering. This
    /// consumes no operands and does not reconstruct argument/loan delivery.
    pub(super) fn map_call_birth(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let origin = self.expression_span(expression)?;
        let ty = self.expression_ssa_type(expression, span)?;
        let instruction = unique(self.function.instructions.iter().filter(|instruction| {
            instruction.origin == Origin::Source(origin)
                && matches!(
                    instruction.operation,
                    Operation::DirectCall { .. }
                        | Operation::MapConstruct { .. }
                        | Operation::MapConsume { .. }
                )
        }))
        .ok_or_else(missing)?;
        let [EntityId::Value(owner)] = instruction.results.as_slice() else {
            return Err(missing());
        };
        if self
            .function
            .entity(EntityId::Value(*owner))
            .map(|data| (data.ty, data.definition))
            != Some((
                EntityType::Value(ty),
                Definition::InstructionResult {
                    instruction: instruction.id,
                    index: 0,
                },
            ))
        {
            return Err(missing());
        }
        if let Some(construction) = self.typed.map_construction(expression) {
            if construction.expression() != expression
                || self.typed.expression_type(expression) != Some(construction.map_type())
                || instruction.operation != (Operation::MapConstruct { map_type: ty })
            {
                return Err(missing());
            }
        } else if let Some(consume) = self.typed.map_consume(expression) {
            let Operation::MapConsume {
                owner: input,
                map_type,
            } = instruction.operation
            else {
                return Err(missing());
            };
            if consume.expression() != expression
                || map_type != ty
                || self.typed.expression_type(expression) != Some(consume.result_type())
                || self
                    .function
                    .entity(EntityId::Value(input))
                    .map(|data| data.ty)
                    != Some(EntityType::Value(
                        self.expression_ssa_type(consume.receiver(), span)?,
                    ))
            {
                return Err(missing());
            }
        } else {
            let call = self.typed.call(expression).ok_or_else(missing)?;
            let CallableTarget::Source(target) = call.target() else {
                return Err(missing());
            };
            let instance = if let Some(source) = self.source_token {
                self.instance_plan
                    .call_site(source, expression)
                    .cloned()
                    .ok_or_else(missing)?
            } else {
                let type_arguments = call
                    .instance()
                    .type_arguments()
                    .iter()
                    .map(|ty| self.resolve_type(*ty, span))
                    .collect::<Result<Vec<_>, _>>()?;
                FunctionInstanceKey::new(target, type_arguments)
            };
            let expected = *self.function_ids.get(&instance).ok_or_else(missing)?;
            if instance.symbol() != target
                || call.expression() != expression
                || call.result_source() != lang_frontend::type_checking::CallableResultSource::Owned
                || self.typed.expression_type(expression) != Some(call.return_type())
                || !matches!(&instruction.operation, Operation::DirectCall {
                    callee, receiver: None, arguments,
                } if *callee == expected && arguments.len() == call.arguments().len())
            {
                return Err(missing());
            }
        }
        Ok(*owner)
    }

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
