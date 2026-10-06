//! 同步 shared 实参的 frontend loan 验证与 place 准备。
use super::*;
use crate::ssa::model::PlaceId;
use lang_frontend::ownership_checking::{LoanKind as FrontendLoanKind, UnitLoanTarget};

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
        if let Some(loan) =
            self.lower_borrowed_heap_field_loan(argument, fact.target(), target, fact.begin_span())?
        {
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
            Expression::Name => Ok(self.references.get(&span_key(node.span())).copied()),
            Expression::Group { expression } => self.direct_name_symbol(*expression, span),
            _ => Ok(None),
        }
    }
}
