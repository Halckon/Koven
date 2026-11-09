//! Short-circuit flow, with constant execution choices only in the constant capability.
use super::super::constant::{UnitShortCircuitPlan, UnitShortCircuitRhs};
use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{
    ast::ExpressionId,
    parser::{BinaryOperator, Expression, LiteralKind},
    type_checking::ConstValue,
};

impl Checker<'_> {
    pub(super) fn short_circuit_plan(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<UnitShortCircuitPlan>, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = node.payload()
        else {
            return Ok(None);
        };
        let rhs_branch = match operator {
            BinaryOperator::LogicalAnd => 0,
            BinaryOperator::LogicalOr => 1,
            _ => return Ok(None),
        };
        let known = if self.constant_control {
            let mut operand = *left;
            loop {
                if let Some(usage) = self.constant_use(operand) {
                    break match usage.value() {
                        ConstValue::Boolean(value) => Some(*value),
                        _ => None,
                    };
                }
                match self.parsed.ast().expressions().get(operand)?.payload() {
                    Expression::Literal(LiteralKind::Boolean(value)) => break Some(*value),
                    Expression::Group { expression } => operand = *expression,
                    _ => break None,
                }
            }
        } else {
            None
        };
        let rhs = match known {
            Some(value) if value == (rhs_branch == 0) => UnitShortCircuitRhs::Always,
            Some(_) => UnitShortCircuitRhs::Never,
            None if !self.constant_control => {
                let (yes, no) = self.ordinary_condition_outcomes(*left);
                let (reaches_rhs, skips_rhs) = if rhs_branch == 0 {
                    (yes, no)
                } else {
                    (no, yes)
                };
                if !reaches_rhs {
                    UnitShortCircuitRhs::Never
                } else if !skips_rhs
                    || self
                        .typed
                        .ordinary_rhs_required(self.unit_expression(expression))
                    || self
                        .typed
                        .expression_falls_through(self.unit_expression(expression))
                        == Some(false)
                {
                    UnitShortCircuitRhs::Always
                } else {
                    UnitShortCircuitRhs::Conditional
                }
            }
            None => UnitShortCircuitRhs::Conditional,
        };
        Ok(Some(UnitShortCircuitPlan {
            expression: self.unit_expression(expression),
            left: self.unit_expression(*left),
            right: self.unit_expression(*right),
            rhs,
            rhs_branch,
        }))
    }

    /// The constant capability keeps its independent control plan.
    pub(super) fn ordinary_condition_outcomes(&self, expression: ExpressionId) -> (bool, bool) {
        if self.constant_control {
            (true, true)
        } else {
            self.typed
                .ordinary_boolean_outcomes(self.unit_expression(expression))
        }
    }

    pub(super) fn check_short_circuit(
        &mut self,
        plan: UnitShortCircuitPlan,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        if self.constant_control {
            self.short_circuits.insert(plan.expression, plan);
        }
        let mut flows =
            self.check_expression(plan.left.expression(), state, ExpressionUse::Read)?;
        match plan.rhs {
            UnitShortCircuitRhs::Never => Ok(flows),
            UnitShortCircuitRhs::Always => {
                self.chain_expression(flows, plan.right.expression(), ExpressionUse::Read)
            }
            UnitShortCircuitRhs::Conditional => {
                if let Some(next) = flows.next.clone() {
                    flows.merge(self.check_expression(
                        plan.right.expression(),
                        next,
                        ExpressionUse::Read,
                    )?);
                }
                Ok(flows)
            }
        }
    }
}
