use super::{
    AccessKind, ActiveLoan, ActiveLoanOwner, ActiveLoanTarget, Checker, ExpressionUse, Flows,
    OwnershipCheckingError, State,
};
use crate::{
    ast::ExpressionId,
    ownership_checking::{LoanKind, UnitLoanFact, UnitLoanTarget},
    parser::{BinaryOperator, Expression, LiteralKind},
    type_checking::{
        BuiltinType, UnitAggregateProjectionKind, UnitStringOperationDescriptor, UnitTypeKind,
    },
};
impl Checker<'_> {
    pub(super) fn check_string_operation(
        &mut self,
        descriptor: UnitStringOperationDescriptor,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        self.check_shared_receiver_read(descriptor.expression(), descriptor.receiver(), state)
    }

    pub(super) fn uses_string_binary_operands(
        &self,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
    ) -> bool {
        let eq = matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual);
        if !eq && operator != BinaryOperator::Add {
            return false;
        }
        let operand = |mut id| {
            let original = self.unit_expression(id);
            loop {
                match self.parsed.ast().expressions().get(id).ok()?.payload() {
                    Expression::Group { expression } => id = *expression,
                    Expression::Literal(LiteralKind::Null) => return None,
                    _ => break,
                }
            }
            self.typed
                .expression_type(original)
                .and_then(|ty| self.typed.types().get(ty))
        };
        let string = |kind: Option<&UnitTypeKind>| match kind {
            Some(UnitTypeKind::Builtin(BuiltinType::String)) => true,
            Some(UnitTypeKind::Nullable(inner)) if eq => {
                self.typed.types().get(*inner) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
            }
            _ => false,
        };
        let (left, right) = (operand(left), operand(right));
        // A generic nullable can acquire MoveOnly string storage at instantiation.
        // Plan cleanup in the template before the backend chooses that representation.
        let nullable_text_or_generic = |kind: Option<&UnitTypeKind>| {
            matches!(kind, Some(UnitTypeKind::Nullable(inner))
                if matches!(self.typed.types().get(*inner),
                    Some(UnitTypeKind::Builtin(BuiltinType::String) | UnitTypeKind::TypeParameter(_) | UnitTypeKind::StaticSelf(_))))
        };
        string(left)
            && (string(right) || eq && right == Some(&UnitTypeKind::Builtin(BuiltinType::Nothing)))
            || eq
                && ((nullable_text_or_generic(left)
                    && self.is_nullable_builtin(right, BuiltinType::Nothing))
                    || (self.is_nullable_builtin(left, BuiltinType::Nothing)
                        && nullable_text_or_generic(right)))
    }

    pub(super) fn is_null_only_expression(&self, expression: ExpressionId) -> bool {
        let kind = self
            .typed
            .expression_type(self.unit_expression(expression))
            .and_then(|ty| self.typed.types().get(ty));
        self.is_nullable_builtin(kind, BuiltinType::Nothing)
    }

    fn is_nullable_builtin(&self, kind: Option<&UnitTypeKind>, builtin: BuiltinType) -> bool {
        matches!(kind, Some(UnitTypeKind::Nullable(inner))
            if self.typed.types().get(*inner) == Some(&UnitTypeKind::Builtin(builtin)))
    }

    pub(super) fn check_binary_read(
        &mut self,
        binary: ExpressionId,
        left: ExpressionId,
        right: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let before = self.diagnostics.len();
        let mut flows = self.check_expression(left, state, ExpressionUse::Read)?;
        let owner = ActiveLoanOwner::StringBinary(self.unit_expression(binary));
        if let Some(next) = flows.next.as_mut()
            && self.diagnostics.len() == before
            && !self.is_null_only_expression(left)
        {
            let (field, target) = match self.string_binary_field_target(left)? {
                Some(UnitLoanTarget::Place(place)) => (true, Some(place)),
                Some(field) => {
                    self.publish_string_binary_field_loan(binary, left, field)?;
                    (false, None)
                }
                None => (false, self.place(left)?.filter(|place| place.is_root())),
            };
            if let Some(place) = target {
                // The operator reads the completed left operand after evaluating the right.
                let target = self.canonical_borrow_place(&place, next);
                if field {
                    self.publish_string_binary_field_loan(
                        binary,
                        left,
                        UnitLoanTarget::Place(target.clone()),
                    )?;
                }
                next.loans.push(ActiveLoan {
                    owner,
                    target: ActiveLoanTarget::Place(target),
                    kind: LoanKind::Shared,
                    reserved: false,
                    origin: self.parsed.ast().expressions().get(left)?.span(),
                });
            }
        }
        let mut flows = self.chain_expression(flows, right, ExpressionUse::Read)?;
        if let Some(next) = flows.next.as_ref()
            && self.diagnostics.len() == before
            && let Some(target) = self.string_binary_field_target(right)?
        {
            let target = match target {
                UnitLoanTarget::Place(place) => {
                    UnitLoanTarget::Place(self.canonical_borrow_place(&place, next))
                }
                other => other,
            };
            self.publish_string_binary_field_loan(binary, right, target)?;
        }
        for edge in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            edge.loans.retain(|loan| loan.owner != owner);
        }
        Ok(flows)
    }

    pub(super) fn string_binary_field_target(
        &self,
        mut operand: ExpressionId,
    ) -> Result<Option<UnitLoanTarget>, OwnershipCheckingError> {
        while let Expression::Group { expression } =
            self.parsed.ast().expressions().get(operand)?.payload()
        {
            operand = *expression;
        }
        let id = self.unit_expression(operand);
        let Some(projection) = self.typed.aggregate_projection(id) else {
            return Ok(None);
        };
        let kind = self.typed.types().get(projection.ty());
        let nullable = self.is_nullable_builtin(kind, BuiltinType::String);
        if projection.kind() != UnitAggregateProjectionKind::Field
            || !(nullable || kind == Some(&UnitTypeKind::Builtin(BuiltinType::String)))
        {
            return Ok(None);
        }
        if let Some(place) = self
            .place(operand)?
            .filter(|place| place.element().is_none())
        {
            return Ok(Some(UnitLoanTarget::Place(place)));
        }
        if nullable && let Some(parent) = self.temporary_field_owner(operand)? {
            let mut parent = parent.expression();
            while let Expression::Group { expression } =
                self.parsed.ast().expressions().get(parent)?.payload()
            {
                parent = *expression;
            }
            return Ok(Some(UnitLoanTarget::Temporary(
                self.unit_expression(parent),
            )));
        }
        Ok(None)
    }

    fn publish_string_binary_field_loan(
        &mut self,
        binary: ExpressionId,
        operand: ExpressionId,
        target: UnitLoanTarget,
    ) -> Result<(), OwnershipCheckingError> {
        self.push_loan(UnitLoanFact::new(
            self.unit_expression(binary),
            self.unit_expression(operand),
            target,
            LoanKind::Shared,
            self.parsed.ast().expressions().get(operand)?.span(),
            self.parsed.ast().expressions().get(binary)?.span(),
            None,
        ));
        Ok(())
    }

    /// 发布同步读取的短 shared loan，复用 receiver 的既有 owner 身份。
    pub(super) fn check_shared_receiver_read(
        &mut self,
        expression: crate::type_checking::UnitExpressionId,
        receiver: crate::type_checking::UnitExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        if receiver.source_unit() != self.source_unit {
            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.source_unit.index(),
                expression: receiver.expression().index(),
            });
        }
        let before = self.diagnostics.len();
        let mut flows = self.check_expression(
            receiver.expression(),
            state,
            ExpressionUse::Place {
                parameter_span: None,
            },
        )?;
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        if self.diagnostics.len() != before {
            return Ok(flows);
        }
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(receiver.expression())?
            .span();
        let target = if let Some(place) = self.loan_place(receiver.expression())? {
            if !self.access_place(&place, AccessKind::SharedLoan, span, None, state)? {
                return Ok(flows);
            }
            UnitLoanTarget::Place(place)
        } else if let Some(owner) = self
            .temporary_projection_owner(receiver.expression())?
            .or(self.temporary_expression_origin(receiver.expression())?)
        {
            UnitLoanTarget::Temporary(owner)
        } else {
            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.source_unit.index(),
                expression: receiver.expression().index(),
            });
        };
        self.push_loan(UnitLoanFact::new(
            expression,
            receiver,
            target,
            LoanKind::Shared,
            span,
            self.parsed
                .ast()
                .expressions()
                .get(expression.expression())?
                .span(),
            None,
        ));
        Ok(flows)
    }
}
