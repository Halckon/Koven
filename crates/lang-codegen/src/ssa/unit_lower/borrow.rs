//! 同步 shared 实参的 frontend loan 验证与 place 准备。
mod nullable_field;

use super::*;
use crate::ssa::model::PlaceId;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::ownership_checking::{LoanKind as FrontendLoanKind, UnitLoanTarget};
use lang_frontend::type_checking::{CallableResultSource, UnitCallTarget};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_borrow_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        target: SsaTypeId,
        span: Span,
        call_span: Span,
    ) -> Result<(LoanId, Vec<LoanId>, Span), LoweringError> {
        let argument_id = UnitExpressionId::new(self.source_unit, argument);
        if let Some((loan, mut sources)) =
            self.lower_nested_borrow_argument(call, argument, span)?
        {
            if self
                .function
                .entity(EntityId::Loan(loan))
                .map(|data| data.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                })
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            sources.push(loan);
            return Ok((loan, sources, call_span));
        }
        if self.owned.borrow_results().range_uses().iter().any(|fact| {
            fact.expression() == argument_id
                && fact.site() == lang_frontend::ownership_checking::RangeUseSite::Call(call)
        }) {
            let loan = self.lower_short_range(argument, target, span)?;
            return Ok((
                loan,
                self.short_range_call_loans(argument, loan, span)?,
                call_span,
            ));
        }
        let mut facts = self
            .owned
            .loans()
            .iter()
            .filter(|fact| fact.call() == call && fact.argument() == argument_id);
        let Some(fact) = facts.next() else {
            return self
                .lower_known_function_borrow(call, argument, target, span, call_span)?
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if facts.next().is_some()
            || fact.kind() != FrontendLoanKind::Shared
            || fact.end_span() != call_span
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if let Some(loan) = self.lower_nullable_when_borrow(call, argument, fact, target, span)? {
            return Ok((loan, Vec::new(), fact.end_span()));
        }
        if let Some(loan) =
            self.lower_nullable_root_borrow(argument, fact.target(), target, span)?
        {
            return Ok((loan, Vec::new(), fact.end_span()));
        }
        if let Some(proof) =
            self.lower_owned_map_borrow(call, argument, fact.target(), target, span)?
        {
            return Ok((proof, vec![proof], fact.end_span()));
        }
        if matches!(
            self.ssa_types.get(target.index()),
            Some(SsaTypeKind::RangeView { .. })
        ) && let Some(symbol) = self.direct_name_symbol(argument, span)?
            && let Some(LoweredValue::Value(value)) = self.bindings.get(&symbol).copied()
        {
            let (_, place) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::RootPlace { owner: value },
                    vec![EntityType::Place(target)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let [EntityId::Place(place)] = place.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            let (_, loan) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::BorrowBegin {
                        place: *place,
                        kind: LoanKind::Shared,
                    },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    }],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let [EntityId::Loan(loan)] = loan.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            return Ok((*loan, vec![*loan], fact.end_span()));
        }
        if let Some(symbol) = self.direct_name_symbol(argument, span)?
            && self
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .any(|binding| binding.binding() == symbol && binding.origin() == fact.target())
            && let Some(loan) = self.borrow_bindings.get(&symbol).copied()
        {
            return Ok((loan, Vec::new(), fact.end_span()));
        }
        if let Some(loan) = self.lower_borrowed_field_loan(
            call,
            argument,
            fact.target(),
            target,
            fact.begin_span(),
        )? {
            return Ok((loan, vec![loan], fact.end_span()));
        }
        if let Some((loan, created)) =
            self.lower_this_field_borrow(argument, fact.target(), target, fact.begin_span())?
        {
            return Ok((loan, created, fact.end_span()));
        }
        if let UnitLoanTarget::This(owner) = fact.target() {
            return self
                .lower_this_borrow_argument(
                    *owner,
                    argument,
                    target,
                    fact.begin_span(),
                    fact.end_span(),
                )
                .map(|(loan, created, end_span)| {
                    (
                        loan,
                        if created { vec![loan] } else { Vec::new() },
                        end_span,
                    )
                });
        }
        if let UnitLoanTarget::Place(place) = fact.target()
            && place.is_root()
            && self.direct_name_symbol(argument, span)? == Some(place.root())
            && let Some(loan) = self.borrow_bindings.get(&place.root()).copied()
        {
            let expected = EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            };
            if self
                .function
                .entity(EntityId::Loan(loan))
                .map(|data| data.ty)
                != Some(expected)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            return Ok((loan, Vec::new(), fact.end_span()));
        }
        if let Some(loan) = self.lower_while_proof_field_borrow(argument, fact, target, span)? {
            return Ok((loan, vec![loan], fact.end_span()));
        }
        let previous = self.closure_binding_context;
        self.closure_binding_context |= self.typed.expression_type(argument_id).is_some_and(|ty| {
            matches!(
                self.typed.types().get(ty),
                Some(UnitTypeKind::Function { .. })
            )
        });
        let place = self.lower_borrow_place(argument, fact.target(), target, span);
        self.closure_binding_context = previous;
        let place = place?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(fact.begin_span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, fact.begin_span()))?;
        let EntityId::Loan(loan) = results[0] else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                fact.begin_span(),
            ));
        };
        Ok((loan, vec![loan], fact.end_span()))
    }

    /// Qualify the existing synchronous call path without emitting or carrying loans.
    pub(super) fn can_call_with_nullable_root_proof(
        &self,
        call: UnitExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        span: Span,
    ) -> Result<bool, LoweringError> {
        if !self
            .nullable_proofs
            .values()
            .any(|proof| proof.if_scope().is_some())
            || self.thunk_expression.is_some()
            || !self.pending_call_frames.is_empty()
            || !self.pending_operands.is_empty()
            || !self.temporaries.is_empty()
            || self.ensure_while_proof_non_call_carriers(span).is_err()
            || self.typed.construction(call).is_some()
            || self.typed.ownership_primitive(call).is_some()
        {
            return Ok(false);
        }
        let Some(descriptor) = self.typed.call(call) else {
            return Ok(false);
        };
        if descriptor.receiver().is_some()
            || descriptor.aborts()
            || descriptor.prints_line()
            || descriptor.result_source() != CallableResultSource::Owned
            || !matches!(
                descriptor.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            )
            || arguments.is_empty()
            || descriptor
                .arguments()
                .iter()
                .any(|mapping| mapping.mode() != ParameterMode::Borrow)
        {
            return Ok(false);
        }
        let return_type = resolve_concrete_type(
            self.typed,
            descriptor.return_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        if self.typed.copyability(return_type) != Copyability::Copyable
            || builtin_type(self.typed, return_type) == Some(BuiltinType::Nothing)
        {
            return Ok(false);
        }
        let key = self
            .source_plan
            .source(self.source_token)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let route = self
            .source_plan
            .call_site(self.source_token, call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if call.source_unit() != self.source_unit
            || descriptor.expression() != call
            || arguments.len() != descriptor.arguments().len()
            || self.function_ids.get(key) != Some(&self.function.id())
            || !self.function_ids.contains_key(route.key())
            || super::map_iteration::enclosing_lambda(self.parsed, span).is_some()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let mut uses_if_proof = false;
        for (index, argument) in arguments.iter().enumerate() {
            let mapping = descriptor
                .arguments()
                .iter()
                .find(|mapping| mapping.argument_index() == index)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            let argument_id = UnitExpressionId::new(self.source_unit, argument.value);
            let mut facts = self
                .owned
                .loans()
                .iter()
                .filter(|fact| fact.call() == call && fact.argument() == argument_id);
            let Some(fact) = facts.next() else {
                return Ok(false);
            };
            if facts.next().is_some()
                || fact.kind() != FrontendLoanKind::Shared
                || fact.end_span() != span
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
            let UnitLoanTarget::Place(path) = fact.target() else {
                return Ok(false);
            };
            if !path.is_root() {
                return Ok(false);
            }
            let Some(proof) = self.nullable_proofs.get(&path.root()) else {
                return Ok(false);
            };
            let parameter_type = resolve_concrete_type(
                self.typed,
                mapping.parameter_type(),
                self.substitutions,
                self.static_self,
                argument.span,
            )?;
            let target = self
                .type_ids
                .get(&parameter_type)
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            if self
                .lower_nullable_root_borrow(argument.value, fact.target(), target, argument.span)?
                .is_none()
            {
                return Ok(false);
            }
            uses_if_proof |= proof.if_scope().is_some();
        }
        Ok(uses_if_proof)
    }

    /// The current proof owns this loan; the call must not end it as a child.
    fn lower_nullable_root_borrow(
        &self,
        argument: ExpressionId,
        loan_target: &UnitLoanTarget,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        let UnitLoanTarget::Place(path) = loan_target else {
            return Ok(None);
        };
        if !path.is_root() {
            return Ok(None);
        }
        let root = path.root();
        let Some(proof) = self.nullable_proofs.get(&root).copied() else {
            return Ok(None);
        };
        if proof.when_scope().is_some() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let owner_type = self
            .function
            .entity(EntityId::Value(proof.owner))
            .map(|data| data.ty);
        if !matches!(owner_type, Some(EntityType::Value(storage))
            if matches!(self.ssa_types.get(storage.index()), Some(SsaTypeKind::NullableHandle { inner })
                if matches!(self.ssa_types.get(inner.index()), Some(SsaTypeKind::HeapOwner { .. }))))
            || super::map_borrow::owned_remove_origin(self.function, proof.owner)
        {
            return Ok(None);
        }
        let mut operand = UnitExpressionId::new(self.source_unit, argument);
        let Some(use_fact) = self.typed.non_null_use(operand) else {
            return Ok(None);
        };
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        let storage = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete =
            |ty| resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span);
        let declared = concrete(storage)?;
        let narrowed = concrete(use_fact.narrowed_type())?;
        if root.source_unit() != self.source_unit
            || !matches!(self.typed.types().get(declared), Some(UnitTypeKind::Nullable(inner))
                if *inner == narrowed)
            || self.type_ids.get(&narrowed) != Some(&target)
            || proof.inner != target
            || owner_type != self.type_ids.get(&declared).copied().map(EntityType::Value)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        loop {
            let fact = self
                .typed
                .non_null_use(operand)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if operand.source_unit() != self.source_unit
                || fact.expression() != operand
                || fact.symbol() != root
                || fact.declared_type() != storage
                || fact.narrowed_type() != use_fact.narrowed_type()
                || self.typed.expression_type(operand) != Some(fact.narrowed_type())
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => {
                    operand = UnitExpressionId::new(self.source_unit, *expression);
                }
                Expression::Name if self.references.get(&span_key(node.span())) == Some(&root) => {
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
        }
        Ok(Some(proof.loan))
    }

    fn lower_borrow_place(
        &mut self,
        argument: ExpressionId,
        loan_target: &UnitLoanTarget,
        target: SsaTypeId,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(argument)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.lower_borrow_place(*expression, loan_target, target, span);
        }
        if let Some(operation) = self
            .typed
            .rc_operation(UnitExpressionId::new(self.source_unit, argument))
            && operation.kind() == lang_frontend::type_checking::RcOperationKind::Value
        {
            let receiver = operation.receiver().expression();
            let owner = match self.lower(receiver)? {
                LoweredValue::Value(owner) => EntityId::Value(owner),
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            };
            let (_, places) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::SharedPayloadPlace { owner },
                    vec![EntityType::Place(target)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let EntityId::Place(place) = places[0] else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            return Ok(place);
        }
        if let Some(place) =
            self.lower_borrowed_container_element(argument, loan_target, target, span)?
        {
            return Ok(place);
        }
        if let UnitLoanTarget::Place(place) = loan_target
            && !place.is_root()
        {
            return self.lower_owned_field_borrow_place(argument, place, target, span);
        }
        let owner = match loan_target {
            UnitLoanTarget::Place(place) if place.is_root() => self
                .bindings
                .get(&place.root())
                .and_then(|value| match value {
                    LoweredValue::Value(value) => Some(*value),
                    LoweredValue::Unit | LoweredValue::Diverged => None,
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?,
            UnitLoanTarget::Temporary(temporary) => {
                if temporary.source_unit() != self.source_unit {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                match self.lower(temporary.expression())? {
                    LoweredValue::Value(value) => value,
                    LoweredValue::Unit => {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                    LoweredValue::Diverged => {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                }
            }
            UnitLoanTarget::Place(_) | UnitLoanTarget::This(_) => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        // A flow-refined Copyable query result borrows a copied payload, never
        // reinterprets the presence/payload aggregate as the inner value's place.
        let mut owner = owner;
        if let UnitLoanTarget::Place(place) = loan_target
            && place.is_root()
            && let Some(proof) = self
                .typed
                .non_null_use(UnitExpressionId::new(self.source_unit, argument))
            && self.is_map_result_value(owner)
        {
            let declared = super::resolve_concrete_type(
                self.typed,
                proof.declared_type(),
                self.substitutions,
                self.static_self,
                span,
            )?;
            let narrowed = super::resolve_concrete_type(
                self.typed,
                proof.narrowed_type(),
                self.substitutions,
                self.static_self,
                span,
            )?;
            if self.typed.copyability(narrowed)
                != lang_frontend::type_checking::Copyability::Copyable
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let actual = self
                .function
                .entity(EntityId::Value(owner))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
                .ty
                .semantic_type();
            if proof.symbol() != place.root()
                || self.type_ids.get(&declared) != Some(&actual)
                || self.type_ids.get(&narrowed) != Some(&target)
                || self.map_results.get(&actual) != Some(&target)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            owner = match self.unwrap_map_result(owner, span)? {
                LoweredValue::Value(value) => value,
                _ => return Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
            };
        }
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::RootPlace { owner },
                vec![EntityType::Place(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(place) = results[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(place)
    }

    pub(super) fn direct_name_symbol(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<UnitSymbolId>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Name | Expression::This => {
                Ok(self.references.get(&span_key(node.span())).copied())
            }
            Expression::Group { expression } => self.direct_name_symbol(*expression, span),
            _ => Ok(None),
        }
    }
}
