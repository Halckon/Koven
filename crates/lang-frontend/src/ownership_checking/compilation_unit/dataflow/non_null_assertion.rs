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
        escaping: bool,
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
        let mut actual = operand;
        while let crate::parser::Expression::Group { expression } =
            self.parsed.ast().expressions().get(actual)?.payload()
        {
            actual = *expression;
        }
        let direct_control = matches!(
            self.parsed.ast().expressions().get(actual)?.payload(),
            crate::parser::Expression::If { .. } | crate::parser::Expression::When { .. }
        );
        let mut alias_witnesses = if escaping && !direct_control {
            self.expression_root_symbol(operand)?
                .and_then(|symbol| state.escape_witnesses.bindings.get(&symbol).cloned())
                .unwrap_or_default()
        } else {
            Default::default()
        };
        let flows =
            self.check_expression_with_escape(operand, state, usage, escaping && direct_control)?;
        if let Some(next) = &flows.next
            && escaping
            && self.diagnostics.len() == diagnostic_count
        {
            // Control results can carry diagnostic-only alias provenance even
            // after their real closure holders have reached their release point.
            if let Some(witnesses) = next
                .escape_witnesses
                .results
                .get(&self.unit_expression(operand))
            {
                alias_witnesses.extend(witnesses.iter().copied());
            }
            if !alias_witnesses.is_empty() {
                self.reject_borrowed_closure_roots(id, next, alias_witnesses)?;
            }
        }
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
