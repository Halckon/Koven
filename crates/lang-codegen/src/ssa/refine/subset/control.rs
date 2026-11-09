//! Conservative condition outcomes from published completion facts and short-circuit structure.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::UnitShortCircuitRhs,
    parser::{BinaryOperator, Expression, Statement},
    type_checking::{UnitExpressionId, UnitStatementId},
};

use super::Checker;

impl Checker<'_, '_> {
    fn expression_may_continue(&self, expression: ExpressionId) -> bool {
        self.inputs
            .typed
            .expression_falls_through(self.id(expression))
            != Some(false)
    }

    /// A Boolean leaf may take either normal edge unless the frontend proved it exits.
    pub(super) fn condition_outcomes(&self, expression: ExpressionId) -> (bool, bool) {
        let expression = self.id(expression);
        if self.inputs.constant_owned.is_some() {
            self.inputs.typed.structural_boolean_outcomes(expression)
        } else {
            self.inputs.typed.ordinary_boolean_outcomes(expression)
        }
    }

    pub(super) fn logical_reaches_rhs(
        &self,
        control: UnitExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
    ) -> bool {
        if self
            .inputs
            .constant_owned
            .and_then(|owned| owned.short_circuit_at(control))
            .is_some_and(|plan| plan.rhs() == UnitShortCircuitRhs::Never)
        {
            return false;
        }
        let (yes, no) = self.condition_outcomes(left);
        if operator == BinaryOperator::LogicalAnd {
            yes
        } else {
            no
        }
    }

    pub(super) fn statement_may_continue(&self, statement: StatementId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().statements().get(statement) else {
            return true;
        };
        match node.payload() {
            Statement::Block { elements }
            | Statement::ControlBody { elements }
            | Statement::LambdaBody { elements } => elements
                .iter()
                .all(|&element| self.statement_may_continue(element)),
            Statement::Loop { .. } | Statement::While { .. } => {
                self.inputs
                    .typed
                    .loop_falls_through(UnitStatementId::new(self.inputs.source_unit, statement))
                    != Some(false)
            }
            Statement::Expression { expression } => {
                if !self.expression_may_continue(*expression)
                    || self
                        .expression_type(*expression)
                        .is_some_and(|ty| self.nothing(ty))
                {
                    return false;
                }
                if let Ok(node) = self.inputs.parsed.ast().expressions().get(*expression)
                    && let Expression::If {
                        condition,
                        then_branch,
                        else_branch,
                        ..
                    } = node.payload()
                {
                    let (yes, no) = self.condition_outcomes(*condition);
                    return (yes && self.statement_may_continue(*then_branch))
                        || (no
                            && else_branch
                                .is_none_or(|branch| self.statement_may_continue(branch)));
                }
                true
            }
            _ => true,
        }
    }
}
