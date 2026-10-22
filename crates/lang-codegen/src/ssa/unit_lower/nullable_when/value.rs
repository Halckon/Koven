//! Exact direct-call Observe/Consume contracts; only Consume can mint an inner owner.
use super::*;
use lang_frontend::{
    ownership_checking::{
        LoanKind as FrontendLoanKind, OwnershipBindingKind, UnitCallArgumentOwnershipKind,
        UnitLoanFact, UnitLoanTarget, UnitNullableWhenUseFact, UnitNullableWhenUseKind,
        UnitValueDeliveryKind,
    },
    type_checking::{CallableResultSource, UnitCallTarget},
};

struct NullableWhenArgument {
    witness: UnitNullableWhenUseWitness,
    proof: nullable_proof::NullableProof,
    operands: Vec<UnitExpressionId>,
    fact: UnitNullableWhenUseFact,
}

impl UnitExpressionLowerer<'_> {
    fn prepare_nullable_when_argument(
        &self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        mode: ParameterMode,
        span: Span,
    ) -> Result<Option<NullableWhenArgument>, LoweringError> {
        let root = self.direct_name_symbol(argument.expression(), span)?;
        let uses_when = self.typed.nullable_when_use(argument).is_some()
            || root.is_some_and(|root| {
                self.nullable_proofs
                    .get(&root)
                    .is_some_and(|proof| proof.when_scope().is_some())
            });
        if !uses_when {
            return Ok(None);
        }
        let (witness, proof, operands) = self.validate_nullable_when_use(argument, span)?;
        let descriptor = self
            .typed
            .call(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if descriptor.receiver().is_some()
            || descriptor.result_source() != CallableResultSource::Owned
            || descriptor.aborts()
            || descriptor.prints_line()
            || self.thunk_expression.is_some()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let route = self
            .source_plan
            .call_site(self.source_token, call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if call.source_unit() != self.source_unit
            || descriptor.expression() != call
            || route.key().target() != UnitCallableTarget::Declaration(target)
            || !self.function_ids.contains_key(route.key())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if !route.delegation().is_empty() || !route.key().callable_arguments().is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let call_node = self
            .parsed
            .ast()
            .expressions()
            .get(call.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call { arguments, .. } = call_node.payload() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let mut actuals = arguments.iter().enumerate().filter(|(_, actual)| {
            UnitExpressionId::new(self.source_unit, actual.value) == argument
        });
        let (index, _) = actuals.next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut mappings = descriptor
            .arguments()
            .iter()
            .filter(|mapping| mapping.argument_index() == index);
        let mapping = mappings
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut contracts = self
            .owned
            .call_argument_contracts_for(call)
            .filter(|contract| contract.argument() == argument);
        let contract = contracts
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let fact = self
            .owned
            .nullable_when_use(argument)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let plan = self
            .owned
            .nullable_when(witness.control())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(argument.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let expected = match mode {
            ParameterMode::Value => UnitCallArgumentOwnershipKind::Value,
            ParameterMode::Borrow => UnitCallArgumentOwnershipKind::SharedLoan,
            ParameterMode::Inout => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        if actuals.next().is_some()
            || mappings.next().is_some()
            || contracts.next().is_some()
            || mapping.mode() != mode
            || mapping.crosses_thread()
            || contract.crosses_thread()
            || contract.parameter_index() != mapping.parameter_index()
            || contract.parameter_type() != mapping.parameter_type()
            || contract.category() != ExpressionCategory::Place
            || mapping.category() != contract.category()
            || contract.kind() != expected
            || contract.argument_span() != node.span()
            || contract.call_span() != call_node.span()
            || fact.call() != call
            || fact.argument() != argument
            || fact.reference() != witness.reference()
            || fact.witness() != witness
            || plan.uses().iter().filter(|candidate| *candidate == fact).count() != 1
            || root != Some(witness.symbol())
            || self.bindings.get(&witness.symbol()) != Some(&LoweredValue::Value(proof.owner))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let concrete = |ty| {
            resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)
        };
        let inner = concrete(witness.inner_type())?;
        let nullable = concrete(witness.nullable_type())?;
        if concrete(mapping.parameter_type())? != inner
            || self.type_ids.get(&inner) != Some(&proof.inner)
            || self.function.entity(EntityId::Value(proof.owner)).map(|entity| entity.ty)
                != self.type_ids.get(&nullable).copied().map(EntityType::Value)
            || self.typed.copyability(inner) != Copyability::MoveOnly
            || !matches!(self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. } | SsaTypeKind::SharedOwner { .. }))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        match mode {
            ParameterMode::Borrow => {
                let loan = fact
                    .loan()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let loans = self
                    .owned
                    .loans()
                    .iter()
                    .filter(|loan| loan.call() == call && loan.argument() == argument)
                    .collect::<Vec<_>>();
                if fact.kind() != UnitNullableWhenUseKind::Observe
                    || fact.delivery().is_some()
                    || fact.moved_span().is_some()
                    || self.owned.value_deliveries().iter().any(|delivery| {
                        delivery.call() == call && delivery.argument() == argument
                    })
                    || loans.as_slice() != [loan]
                    || loan.kind() != FrontendLoanKind::Shared
                    || loan.begin_span() != contract.loan_begin_span()
                    || loan.end_span() != call_node.span()
                    || loan.parameter_span() != contract.parameter_span()
                    || !matches!(loan.target(), UnitLoanTarget::Place(path)
                        if path.is_root() && path.root() == witness.symbol())
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
            ParameterMode::Value => {
                let delivery = fact
                    .delivery()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let deliveries = self
                    .owned
                    .value_deliveries()
                    .iter()
                    .filter(|delivery| delivery.call() == call && delivery.argument() == argument)
                    .collect::<Vec<_>>();
                let leaf = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(witness.reference().expression())
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let owned = self.owned.binding_kind(witness.symbol())
                    == Some(OwnershipBindingKind::Owned)
                    || (self.owned.binding_kind(witness.symbol()).is_none()
                        && self.owned.formed_owned_locals().contains(&witness.symbol()));
                if fact.kind() != UnitNullableWhenUseKind::Consume
                    || fact.loan().is_some()
                    || self.owned.loans().iter().any(|loan| {
                        loan.call() == call && loan.argument() == argument
                    })
                    || deliveries.as_slice() != [delivery]
                    || delivery.kind() != UnitValueDeliveryKind::Move
                    || delivery.span() != node.span()
                    || delivery.parameter_span() != contract.parameter_span()
                    || delivery.place().is_none_or(|path| {
                        !path.is_root() || path.root() != witness.symbol()
                    })
                    || fact.moved_span() != Some(leaf.span())
                    || !owned
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
            ParameterMode::Inout => unreachable!("mode was rejected above"),
        }
        Ok(Some(NullableWhenArgument {
            witness,
            proof,
            operands,
            fact: fact.clone(),
        }))
    }

    pub(in super::super) fn lower_nullable_when_borrow(
        &self,
        call: UnitExpressionId,
        argument: ExpressionId,
        fact: &UnitLoanFact,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        if let UnitLoanTarget::Place(path) = fact.target()
            && !path.is_root()
            && self
                .nullable_proofs
                .get(&path.root())
                .is_some_and(|proof| proof.when_scope().is_some())
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let argument = UnitExpressionId::new(self.source_unit, argument);
        let Some(prepared) =
            self.prepare_nullable_when_argument(call, argument, ParameterMode::Borrow, span)?
        else {
            return Ok(None);
        };
        if prepared.fact.loan() != Some(fact) || prepared.proof.inner != target {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // The persistent When scope owns this view. A synchronous callee must not
        // count it as a newly-created child, Take it, or implicitly retain its Rc.
        Ok(Some(prepared.proof.loan))
    }

    pub(in super::super) fn lower_nullable_when_value(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        parameter_type: UnitTypeId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let argument = UnitExpressionId::new(self.source_unit, argument);
        let Some(prepared) =
            self.prepare_nullable_when_argument(call, argument, ParameterMode::Value, span)?
        else {
            return Ok(None);
        };
        let expected = resolve_concrete_type(
            self.typed,
            parameter_type,
            self.substitutions,
            self.static_self,
            span,
        )?;
        if self.type_ids.get(&expected) != Some(&prepared.proof.inner) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_when_retirement(prepared.witness.symbol(), span)?;
        self.preflight_nullable_when_take_drops(&prepared.operands, span)?;
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                Operation::NullableTake {
                    owner: prepared.proof.owner,
                    proof: prepared.proof.loan,
                },
                vec![EntityType::Value(prepared.proof.inner)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let inner = require_value(values[0], span)?;
        // Every fallible fact/storage check preceded Take. It consumes the view;
        // there is deliberately no BorrowEnd, no wrapper Drop and no second transfer.
        self.nullable_proofs.remove(&prepared.witness.symbol());
        self.bindings.remove(&prepared.witness.symbol());
        self.closure_bindings.remove(&prepared.witness.symbol());
        self.temporaries.retain(|_, owner| *owner != prepared.proof.owner);
        // The original Name/Group cleanup points were all checked empty before
        // Take. No fact consumer, including result/closure/Map cleanup, runs here.
        // The original argument frame now registers this inner as its pending Value,
        // and removes that obligation only at the original successful call commit.
        Ok(Some(LoweredValue::Value(inner)))
    }

    pub(in super::super) fn emit_nullable_when_borrow_drops(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
    ) -> Result<bool, LoweringError> {
        let Some(fact) = self.owned.nullable_when_use(argument) else {
            return Ok(false);
        };
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(argument.expression())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        if fact.call() != call || fact.kind() != UnitNullableWhenUseKind::Observe {
            return Err(lowering_error(LoweringErrorKind::MissingFact, node.span()));
        }
        let (_, _, operands) = self.validate_nullable_when_use(argument, node.span())?;
        self.emit_nullable_when_operand_drops(&operands)?;
        Ok(true)
    }
}
