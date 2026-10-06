//! Sealed source addresses use typed, call-scoped ABI owners rather than fabricated LoanFacts.
use super::*;
use lang_frontend::{
    name_resolution::UnitReferenceTarget, ownership_checking::UnitCallableOrigin,
    type_checking::UnitCallableTarget,
};

impl UnitExpressionLowerer<'_> {
    pub(in crate::ssa::unit_lower) fn known_function_address(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<(FunctionId, SsaTypeId)>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.known_function_address(*expression, span);
        }
        if !matches!(node.payload(), Expression::Name) {
            return Ok(None);
        }
        let id = UnitExpressionId::new(self.source_unit, expression);
        let Some(fact) = self.owned.callable_origin(id) else {
            return Ok(None);
        };
        let UnitCallableOrigin::KnownFunction(target) = fact.origin() else {
            return Ok(None);
        };
        // A storage alias retains its ordinary binding/LoanFact route. An overload candidate
        // is accepted only when the published sealed provenance already selected that target.
        let direct = self.names.names().references().iter().any(|reference| {
            reference.source_unit() == self.source_unit
                && reference.span() == node.span()
                && reference.namespace() == Some(Namespace::Value)
                && match (target, reference.target()) {
                    (
                        UnitCallableTarget::Declaration(expected),
                        UnitReferenceTarget::Declaration(actual),
                    ) => expected == *actual,
                    (
                        UnitCallableTarget::Declaration(expected),
                        UnitReferenceTarget::OverloadSet(actual),
                    ) => actual.contains(&expected),
                    (UnitCallableTarget::Symbol(expected), UnitReferenceTarget::Symbol(actual)) => {
                        expected == *actual
                    }
                    _ => false,
                }
        });
        if !direct {
            return Ok(None);
        }
        let ty = self
            .typed
            .expression_type(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let ty = resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)?;
        if !matches!(
            self.typed.types().get(ty),
            Some(UnitTypeKind::Function { .. })
        ) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let key = crate::ssa::unit_plan::UnitFunctionInstanceKey::for_target(target, Vec::new());
        let target = self
            .function_ids
            .get(&key)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let callable = self.callable_abi.expression_type(
            self.source_plan,
            self.typed,
            self.owned,
            self.source_token,
            id,
            span,
        )?;
        Ok(Some((target, callable)))
    }

    pub(in crate::ssa::unit_lower) fn lower_known_function_borrow(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        target: SsaTypeId,
        span: Span,
        call_span: Span,
    ) -> Result<
        Option<(
            crate::ssa::model::LoanId,
            Vec<crate::ssa::model::LoanId>,
            Span,
        )>,
        LoweringError,
    > {
        let Some((function, callable)) = self.known_function_address(argument, span)? else {
            return Ok(None);
        };
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(call.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, call_span))?;
        let Expression::Call { arguments, .. } = node.payload() else {
            return Ok(None);
        };
        let Some(index) = arguments.iter().position(|item| item.value == argument) else {
            return Ok(None);
        };
        let mode = if let Some(descriptor) = self.typed.container_construction(call) {
            descriptor.parameter_modes().get(index).copied()
        } else {
            self.typed.call(call).and_then(|descriptor| {
                descriptor
                    .arguments()
                    .iter()
                    .find(|mapping| mapping.argument_index() == index)
                    .map(|mapping| mapping.mode())
            })
        };
        if mode != Some(ParameterMode::Borrow) || callable != target {
            return Ok(None);
        }
        if !self
            .pending_call_frames
            .last()
            .is_some_and(|frame| frame.call == call && !frame.receiver)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, call_span));
        }
        let (_, owners) = self
            .function
            .append_instruction(
                self.block,
                Operation::FunctionAddress { target: function },
                vec![EntityType::Value(callable)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let owner = require_value(owners[0], span)?;
        let (_, places) = self
            .function
            .append_instruction(
                self.block,
                Operation::RootPlace { owner },
                vec![EntityType::Place(callable)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(place) = places[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, loans) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: callable,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let owner_slot = self.pending_operands.len();
        self.pending_operands.push(EntityId::Value(owner));
        let loan_slot = self.pending_operands.len();
        self.pending_operands.push(EntityId::Loan(loan));
        self.pending_call_frames
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .abi_slots
            .push((owner_slot, loan_slot));
        Ok(Some((loan, vec![loan], call_span)))
    }
}
