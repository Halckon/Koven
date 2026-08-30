//! compilation-unit source call 的 Value delivery 与同步 shared-Borrow lowering。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        LoanKind as FrontendLoanKind, UnitDropPoint, UnitLoanTarget, UnitValueDeliveryKind,
        UnitValueDeliverySource,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, Copyability, ExpressionCategory, ParameterMode, UnitCallTarget,
        UnitExpressionId,
    },
};

use super::{
    LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error, require_value,
    resolve_concrete_type, span_key,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        EntityId, EntityType, LoanId, LoanKind, Operation, Origin, PlaceAccess, PlaceId, SsaTypeId,
    },
    unit_plan::UnitFunctionInstanceKey,
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_name(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let symbol = self
            .references
            .get(&span_key(span))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let Some(value) = self.bindings.get(&symbol).copied() {
            return Ok(value);
        }
        let loan = self
            .borrow_bindings
            .get(&symbol)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let id = UnitExpressionId::new(self.source_unit, expression);
        let ty = self
            .typed
            .types()
            .expression_type(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.types().copyability(ty) != Copyability::Copyable {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                },
                vec![EntityType::Value(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    pub(super) fn lower_call(
        &mut self,
        expression: ExpressionId,
        callee_expression: ExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let call = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .types()
            .call(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.target() == UnitCallTarget::FunctionValue {
            return self.lower_function_value_call(
                expression,
                callee_expression,
                arguments,
                descriptor,
                span,
            );
        }
        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if arguments.len() != descriptor.arguments().len()
            || descriptor
                .arguments()
                .iter()
                .any(|argument| argument.mode() == ParameterMode::Inout)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let type_arguments = descriptor
            .instance()
            .type_arguments()
            .iter()
            .map(|ty| resolve_concrete_type(self.typed, *ty, self.substitutions, span))
            .collect::<Result<Vec<_>, _>>()?;
        let callee = self
            .function_ids
            .get(&UnitFunctionInstanceKey::new(target, type_arguments))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut ordered = vec![None; descriptor.arguments().len()];
        let mut created_loans = Vec::new();
        for (argument_index, argument) in arguments.iter().enumerate() {
            let mapping = descriptor
                .arguments()
                .iter()
                .find(|mapping| mapping.argument_index() == argument_index)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            let entity = match mapping.mode() {
                ParameterMode::Value => {
                    match self.lower_value_argument(call, argument.value, argument.span)? {
                        LoweredValue::Value(value) => EntityId::Value(value),
                        LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                        LoweredValue::Unit => {
                            return Err(lowering_error(
                                LoweringErrorKind::InvalidModel,
                                argument.span,
                            ));
                        }
                    }
                }
                ParameterMode::Borrow => {
                    let parameter_type = resolve_concrete_type(
                        self.typed,
                        mapping.parameter_type(),
                        self.substitutions,
                        argument.span,
                    )?;
                    let target = self.type_ids.get(&parameter_type).copied().ok_or_else(|| {
                        lowering_error(LoweringErrorKind::MissingFact, argument.span)
                    })?;
                    let (loan, created, end_span) = self.lower_borrow_argument(
                        call,
                        argument.value,
                        target,
                        argument.span,
                        span,
                    )?;
                    if created {
                        created_loans.push((loan, end_span));
                    }
                    EntityId::Loan(loan)
                }
                ParameterMode::Inout => {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        argument.span,
                    ));
                }
            };
            let slot = ordered
                .get_mut(mapping.parameter_index())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            if slot.replace(entity).is_some() {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
        }
        let arguments = ordered
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let return_type = resolve_concrete_type(
            self.typed,
            descriptor.return_type(),
            self.substitutions,
            span,
        )?;
        let result_types = if builtin_type(self.typed, return_type) == Some(BuiltinType::Unit) {
            Vec::new()
        } else {
            vec![EntityType::Value(
                *self
                    .type_ids
                    .get(&return_type)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
            )]
        };
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::DirectCall { callee, arguments },
                result_types,
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        for (loan, end_span) in created_loans.into_iter().rev() {
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    Origin::Source(end_span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, end_span))?;
        }
        self.emit_drops(UnitDropPoint::CallReturn(call))?;
        match results.as_slice() {
            [] => Ok(LoweredValue::Unit),
            [result] => Ok(LoweredValue::Value(require_value(*result, span)?)),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    fn lower_value_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let value = match self.lower(argument)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Unit => self.materialize_unit_value(argument, span)?,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
        };
        let argument = UnitExpressionId::new(self.source_unit, argument);
        let mut deliveries = self
            .owned
            .ownership()
            .value_deliveries()
            .iter()
            .filter(|delivery| delivery.call() == call && delivery.argument() == argument);
        let delivery = deliveries
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let ty = self
            .typed
            .types()
            .expression_type(argument)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let expected = match (
            self.typed.types().expression_category(argument),
            self.typed.types().copyability(ty),
        ) {
            (Some(ExpressionCategory::Temporary), _) => UnitValueDeliveryKind::Temporary,
            (Some(ExpressionCategory::Place), Copyability::Copyable) => UnitValueDeliveryKind::Copy,
            (Some(ExpressionCategory::Place), Copyability::MoveOnly) => UnitValueDeliveryKind::Move,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if deliveries.next().is_some() || delivery.kind() != expected {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        match expected {
            UnitValueDeliveryKind::Copy => {}
            UnitValueDeliveryKind::Move => {
                let place = delivery
                    .place()
                    .filter(|place| place.is_root())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                self.take_owned_binding(place.root(), value, span)?;
            }
            UnitValueDeliveryKind::Temporary => {
                if delivery.source() != &UnitValueDeliverySource::Temporary(argument) {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                if self.typed.types().copyability(ty) == Copyability::MoveOnly {
                    self.take_owned_temporary(value, span)?;
                }
            }
        }
        Ok(LoweredValue::Value(value))
    }

    fn lower_borrow_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        target: SsaTypeId,
        span: Span,
        call_span: Span,
    ) -> Result<(LoanId, bool, Span), LoweringError> {
        let argument_id = UnitExpressionId::new(self.source_unit, argument);
        let mut facts = self
            .owned
            .ownership()
            .loans()
            .iter()
            .filter(|fact| fact.call() == call && fact.argument() == argument_id);
        let fact = facts
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if facts.next().is_some()
            || fact.kind() != FrontendLoanKind::Shared
            || fact.end_span() != call_span
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
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
            return Ok((loan, false, fact.end_span()));
        }
        let place = self.lower_borrow_place(argument, fact.target(), target, span)?;
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
        Ok((loan, true, fact.end_span()))
    }

    fn lower_borrow_place(
        &mut self,
        argument: ExpressionId,
        loan_target: &UnitLoanTarget,
        target: SsaTypeId,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        if let Some(place) =
            self.lower_borrowed_container_element(argument, loan_target, target, span)?
        {
            return Ok(place);
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
            UnitLoanTarget::Place(_) => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
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

    fn direct_name_symbol(
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
            Expression::Name => Ok(self.references.get(&span_key(node.span())).copied()),
            Expression::Group { expression } => self.direct_name_symbol(*expression, span),
            _ => Ok(None),
        }
    }
}
