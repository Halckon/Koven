//! 从 validated unit descriptor 登记成功提取，不为 null Abort 构造清理边。
use super::{Checker, ExpressionUse, Flows, State};
use crate::{
    ast::ExpressionId,
    ownership_checking::{
        NonNullAssertionTransferKind, OwnershipCheckingError, UnitNonNullAssertionOwnershipPlan,
    },
    type_checking::{Copyability, UnitExpressionId},
};

impl Checker<'_> {
    pub(super) fn check_non_null_assertion(
        &mut self,
        id: ExpressionId,
        operand: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let key = UnitExpressionId::new(self.source_unit, id);
        let descriptor = self
            .typed
            .non_null_assertion(key)
            .filter(|descriptor| {
                descriptor.operand() == UnitExpressionId::new(self.source_unit, operand)
            })
            .ok_or(OwnershipCheckingError::InvalidUnitNonNullAssertion {
                source_unit: self.source_unit.index(),
                expression: id.index(),
            })?;
        let (usage, transfer) = if descriptor.copyability() == Copyability::MoveOnly {
            (
                ExpressionUse::Consume {
                    parameter_span: None,
                },
                NonNullAssertionTransferKind::Consume,
            )
        } else {
            (ExpressionUse::Read, NonNullAssertionTransferKind::Copy)
        };
        let diagnostic_count = self.diagnostics.len();
        let flows = self.check_expression(operand, state, usage)?;
        if flows.next.is_some() && self.diagnostics.len() == diagnostic_count {
            self.non_null_assertions
                .push(UnitNonNullAssertionOwnershipPlan {
                    descriptor,
                    source_place: self.place(operand)?,
                    non_null_transfer: transfer,
                });
        }
        Ok(flows)
    }
}
