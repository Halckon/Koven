use super::{AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{
    ownership_checking::{LoanKind, UnitLoanFact, UnitLoanTarget},
    type_checking::UnitStringOperationDescriptor,
};
impl Checker<'_> {
    pub(super) fn check_string_operation(
        &mut self,
        descriptor: UnitStringOperationDescriptor,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        self.check_shared_receiver_read(descriptor.expression(), descriptor.receiver(), state)
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
        self.loans.push(UnitLoanFact::new(
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
