//! Constant capability control decisions shared by traversal, liveness and drop planning.
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
        if !self.constant_control {
            return Ok(None);
        }
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
        let mut operand = *left;
        let known = loop {
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
        };
        let rhs = match known {
            Some(value) if value == (rhs_branch == 0) => UnitShortCircuitRhs::Always,
            Some(_) => UnitShortCircuitRhs::Never,
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

    pub(super) fn check_short_circuit(
        &mut self,
        plan: UnitShortCircuitPlan,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        self.short_circuits.insert(plan.expression, plan);
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
