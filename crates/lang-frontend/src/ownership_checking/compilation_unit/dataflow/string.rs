use super::{
    AccessKind, ActiveLoan, ActiveLoanOwner, ActiveLoanTarget, Checker, ExpressionUse, Flows,
    OwnershipCheckingError, State,
};
use crate::{
    ast::ExpressionId,
    ownership_checking::{LoanKind, UnitLoanFact, UnitLoanTarget},
    parser::{BinaryOperator, Expression, LiteralKind},
    type_checking::{BuiltinType, UnitStringOperationDescriptor, UnitTypeKind},
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
        let right = operand(right);
        string(operand(left))
            && (string(right) || eq && right == Some(&UnitTypeKind::Builtin(BuiltinType::Nothing)))
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
            && let Some(place) = self.place(left)?.filter(|place| place.is_root())
        {
            // The operator reads the completed left operand after evaluating the right.
            next.loans.push(ActiveLoan {
                owner,
                target: ActiveLoanTarget::Place(
                    next.borrow_bindings
                        .get(&place.root())
                        .cloned()
                        .unwrap_or(place),
                ),
                kind: LoanKind::Shared,
                reserved: false,
                origin: self.parsed.ast().expressions().get(left)?.span(),
            });
        }
        let mut flows = self.chain_expression(flows, right, ExpressionUse::Read)?;
        for edge in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            edge.loans.retain(|loan| loan.owner != owner);
        }
        Ok(flows)
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
